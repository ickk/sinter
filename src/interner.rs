use {
  crate::{ext::BoxNonNull as _, IStr},
  ::core::{
    cell::{Cell, OnceCell, UnsafeCell},
    iter,
    mem::MaybeUninit,
    ptr,
    sync::atomic::{AtomicPtr, AtomicU32, AtomicUsize, Ordering},
  },
  ::hashbrown::HashTable,
  ::parking_lot::{lock_api::RawMutex as _, RawMutex},
  ::std::thread,
  ::wyhash::wyhash,
};

// TODO check implementation with loom & miri

/// The String Interner instance singleton
pub(crate) static THE_INTERNER: Interner = Interner::new();

thread_local! {
  /// This is an epoch counter for the current thread. It allows the writer to
  /// reliably wait on outstanding reads from id_map_mut
  static LOCAL_EPOCH: Cell<LocalEpoch> = const { Cell::new(LocalEpoch::None) };
}
/// Local epoch counter starting value
const LOCAL_EPOCH_INIT: usize = 2;
/// Local epoch counter gets assigned this value when the thread terminates,
/// effectively transferring ownership of the atomic to the Interner
const LOCAL_EPOCH_DEAD: usize = 0;

#[derive(Debug, Clone)]
enum LocalEpoch {
  Some(ptr::NonNull<AtomicUsize>),
  None,
}

impl Drop for LocalEpoch {
  fn drop(&mut self) {
    if let LocalEpoch::Some(ptr) = self {
      let epoch = unsafe { ptr.as_ref() };
      // mark this counter as dead, so that the Interner can clean it its
      // memory.
      epoch.store(LOCAL_EPOCH_DEAD, Ordering::Relaxed)
    }
  }
}

// safety: memory safety is maintained in a multithreaded context using the
// `write_lock` and other atomics
unsafe impl Sync for Interner {}

/// A thread-safe global string interner
pub(crate) struct Interner {
  /// freely readable* hashtable of `&str`s to unique `IStr`s
  /// readers must (atomically) increment their epoch before and after reading
  id_map: AtomicPtr<HashTable<IStr>>,

  /// reading/writing of all following fields is protected by this lock
  write_lock: RawMutex,

  /// linked list of memory pages, must have write_lock to read/write
  pages: OnceCell<&'static Page>,

  /// the index of the first unused byte of the last memory page
  last_memory_index: AtomicU32,

  /// The writer's (must have lock) version of the id_map.
  /// Additionally must wait on readers to depart (using epoch counters)
  /// atomically swapped with id_map by the writer.
  id_map_mut: AtomicPtr<HashTable<IStr>>,

  /// stores a copy of the last `IStr` added (which may still need to be added
  /// to the other map)
  pending_add: Cell<Option<IStr>>,

  /// references to epoch counters for each thread. Even counters indicate no
  /// reads are happening. Odd counters indicate reads map be happening.
  /// The writer can wait until odd counters increment by at least 1, to be
  /// sure there are no lingering reads on its copy.
  epochs: UnsafeCell<Vec<(thread::ThreadId, ptr::NonNull<AtomicUsize>)>>,
}

pub(crate) const WYHASH_SEED: u64 = 0;
pub(crate) const SIZE_OF_WYHASH: usize = ::core::mem::size_of::<u64>();

impl Interner {
  /// Creates a new Interner
  pub(crate) const fn new() -> Self {
    Interner {
      write_lock: RawMutex::INIT,
      pages: OnceCell::new(),
      last_memory_index: AtomicU32::new(0),
      id_map: AtomicPtr::new(ptr::null_mut()),
      id_map_mut: AtomicPtr::new(ptr::null_mut()),
      pending_add: Cell::new(None),
      epochs: UnsafeCell::new(Vec::new()),
    }
  }

  /// Locklessly find an extant `IStr` corresponding to the string given, if
  /// one exists.
  pub(crate) fn get_interned(&'static self, s: &str) -> Option<IStr> {
    let s_wyhash = wyhash(s.as_bytes(), WYHASH_SEED);
    let (ret, _) = self.get_interned_and_map_len(s, s_wyhash);
    ret
  }

  /// Collect all of the currently interned strings into a collection of type
  /// `B`.
  pub(crate) fn collect_interned_strings<B>(&'static self) -> B
  where
    B: iter::FromIterator<IStr>,
  {
    let local_epoch = self.local_epoch_or_init();

    local_epoch.fetch_add(1, Ordering::Release);
    let ret = 'reading: {
      let id_map = self.id_map.load(Ordering::Acquire);
      if !id_map.is_null() {
        let id_map = unsafe { &*id_map };
        break 'reading B::from_iter(id_map.iter().copied());
      } else {
        break 'reading B::from_iter(iter::empty());
      }
    };
    local_epoch.fetch_add(1, Ordering::Release);

    ret
  }

  /// locklessly try to get the `IStr` corresponding to the `&str` given, if
  /// one exists. Also returns the length of the id_map.
  ///
  /// caveat: not technically lockless if this is the first call to the
  /// interner for this thread (see `Interner::local_epoch_or_init`).
  #[inline]
  fn get_interned_and_map_len(
    &'static self,
    s: &str,
    s_wyhash: u64,
  ) -> (Option<IStr>, usize) {
    let local_epoch = self.local_epoch_or_init();
    let mut id_map_len = 0;
    // search among the existing Ids in the map
    local_epoch.fetch_add(1, Ordering::Release);
    let ret = 'reading: {
      let id_map = self.id_map.load(Ordering::Acquire);
      if !id_map.is_null() {
        let id_map = unsafe { &*id_map };
        id_map_len = id_map.len();
        if let Some(&istr) = id_map.find(s_wyhash, |val| val.0 == s) {
          // we found it!
          break 'reading Some(istr);
        }
      }
      None
    };
    local_epoch.fetch_add(1, Ordering::Release);

    (ret, id_map_len)
  }

  /// local thread initialisation
  #[inline]
  fn local_epoch_or_init(&'static self) -> &AtomicUsize {
    let local_epoch = LOCAL_EPOCH.with(|cell| {
      // Need to get a reference to the value in the cell, but it's not Copy
      // because we want the destructor to run when the thread terminates.
      if let &LocalEpoch::Some(ptr) = unsafe { &*cell.as_ptr() } {
        return unsafe { ptr.as_ref() };
      } else {
        let ptr =
          Box::into_non_null(Box::new(AtomicUsize::new(LOCAL_EPOCH_INIT)));

        LOCAL_EPOCH.set(LocalEpoch::Some(ptr));
        self.write_lock.lock();
        '_holding_lock: {
          let epochs = unsafe { &mut *self.epochs.get() };

          // we prune the dead epochs here, because we're holding the
          // write_lock anyway, and besides we really only need to free them at
          // all if we're creating a lot of threads and then throwing them
          // away.
          // TODO: if this is too slow, we could have another pair of counters.
          // One to count the number of threads created, and another to count
          // the number of threads killed. Then we'd only bother to prune if
          // the difference was greater than the number of epochs in the vec.
          Self::prune_dead_epochs(epochs);

          epochs.push((thread::current().id(), ptr));
        }
        unsafe { self.write_lock.unlock() };

        return unsafe { ptr.as_ref() };
      }
    });

    local_epoch
  }

  /// frees and removes any epoch with a value of `LOCAL_EPOCH_DEAD`
  #[inline]
  fn prune_dead_epochs(
    epochs: &mut Vec<(thread::ThreadId, ptr::NonNull<AtomicUsize>)>,
  ) {
    epochs.retain(|&(_thread_id, ptr)| {
      let epoch = unsafe { ptr.as_ref() };
      if epoch.load(Ordering::Acquire) == LOCAL_EPOCH_DEAD {
        // free the memory for the atomic and remove this entry from the list
        let _ = unsafe { Box::from_non_null(ptr) };
        false
      } else {
        true
      }
    });
  }

  /// Intern a new string, or return the extant [`IStr`] if one exists
  ///
  /// This operation may be slow, depending on whether the string has been
  /// previously interned.
  pub(crate) fn intern(&'static self, s: &str) -> IStr {
    let s_wyhash = wyhash(s.as_bytes(), WYHASH_SEED);

    // see if one already exists
    let (ret, id_map_len) = self.get_interned_and_map_len(s, s_wyhash);
    if let Some(istr) = ret {
      return istr;
    }

    // didn't find it, so acquire a lock and then actually intern a new string
    self.write_lock.lock();
    let ret = 'holding_lock: {
      let mut id_map_mut = self.id_map_mut.load(Ordering::Acquire);

      // check it wasn't just added while we were waiting
      // TODO checking this last value is always slow (not really but requires
      // getting the lock)
      {
        let mut some_pending = 0;
        if let Some(pending_istr) = self.pending_add.get() {
          some_pending = 1;
          if pending_istr.wyhash() == s_wyhash && pending_istr.as_str() == s {
            break 'holding_lock pending_istr;
          }
        }
        // if the id_map_mut differs in length to the id_map we checked earlier
        // then we may need to re-check it. This can happen if we weren't the
        // immediate next lock acquirer
        if !id_map_mut.is_null() {
          let id_map_mut = unsafe { &*id_map_mut };
          if id_map_mut.len() + some_pending > id_map_len {
            if let Some(&istr) = id_map_mut.find(s_wyhash, |val| val.0 == s) {
              break 'holding_lock istr;
            }
          }
        }
      }

      // lazy initialisation of id_map_mut
      if id_map_mut.is_null() {
        id_map_mut = Box::into_raw(Box::new(HashTable::new()));
      }
      let id_map_mut = unsafe { &mut *id_map_mut };

      // iterate all odd epochs until they're no longer odd (i.e. readers are
      // done with this map)
      {
        let epochs = unsafe { &mut *self.epochs.get() };
        let all_epochs = epochs.iter().collect::<Vec<_>>();
        // TODO remove this clone, cache a vec instead, use smallvec
        let mut odd_epochs = all_epochs
          .iter()
          .enumerate()
          .map(|(i, (thread_id, ptr_epoch))| {
            let e = unsafe { ptr_epoch.as_ref() };
            (i, thread_id, e.load(Ordering::Acquire))
          })
          .filter(|(_, _, e)| (e % 2) == 1)
          .collect::<Vec<_>>();
        if odd_epochs.is_empty() {
          let mut spin = 0;
          loop {
            odd_epochs.retain(|&(i, _thread_id, old)| {
              let epoch_i = unsafe { epochs[i].1.as_ref() };
              let new = epoch_i.load(Ordering::Relaxed);
              new == old
            });
            if odd_epochs.is_empty() {
              break;
            }
            // TODO: improve this spin loop, exponential back-off, waiting on
            // src threads to signal for this to continue (parking/unparking)
            ::core::hint::spin_loop();
            if spin > 100 {
              thread::yield_now();
            }
            spin += 1;
          }
        }
      }

      // add the value from last time to this map
      if let Some(pending_istr) = self.pending_add.take() {
        id_map_mut.insert_unique(pending_istr.wyhash(), pending_istr, |v| {
          wyhash(v.as_bytes(), WYHASH_SEED)
        });
      }

      // write the string to memory page
      let interned_str;
      {
        // lazily initialise the first page
        if self.pages.get().is_none() {
          // note: we leave room for a trailing null byte
          let _ = self
            .pages
            .set(Page::with_min_capacity(SIZE_OF_WYHASH + s.len() + 1));
        }
        // find the last page in the deck
        let mut last_page = &self.pages;
        loop {
          let next_page =
            unsafe { last_page.get().unwrap().next_page.assume_init_ref() };
          if next_page.get().is_none() {
            break;
          }
          last_page = next_page;
        }
        let mut last_page = *last_page.get().unwrap();

        let available_bytes = unsafe { (*last_page.mem.get()).len() }
          - self.last_memory_index.load(Ordering::Acquire) as usize;
        if available_bytes < (SIZE_OF_WYHASH + s.len() + 1) {
          // we don't have enough memory to store this string, so create a new
          // page
          // note: we leave room for the trailing null byte and the wyhash
          unsafe {
            last_page.extend_with_new_page(SIZE_OF_WYHASH + s.len() + 1)
          };
          let next_page = unsafe { last_page.next_page.assume_init_ref() };
          last_page = next_page.get().unwrap();
          self.last_memory_index.store(0, Ordering::Release);
        }
        // there's enough bytes available on this page, so store the string
        let hash_index =
          self.last_memory_index.load(Ordering::Acquire) as usize;
        let str_index = hash_index + SIZE_OF_WYHASH;
        let mem = unsafe { &mut *last_page.mem.get() };
        let hash_slice = &mut mem[hash_index..(hash_index + SIZE_OF_WYHASH)];
        hash_slice.copy_from_slice(&s_wyhash.to_ne_bytes());
        let str_slice = &mut mem[str_index..(str_index + s.len())];
        str_slice.copy_from_slice(s.as_bytes());
        // note: we leave room for the trailing null byte
        self
          .last_memory_index
          .store((str_index + s.len() + 1) as u32, Ordering::Release);

        interned_str = IStr(::core::str::from_utf8(str_slice).unwrap());
      }

      // add to id_map
      id_map_mut.insert_unique(s_wyhash, interned_str, |v| {
        wyhash(v.as_bytes(), WYHASH_SEED)
      });

      // cache a copy for the back buffer table
      // we defer it until next time to avoid waiting on the observers
      self.pending_add.set(Some(interned_str));

      // swap the tables
      let id_map = self.id_map.swap(id_map_mut, Ordering::AcqRel);
      self.id_map_mut.swap(id_map, Ordering::Release);

      break 'holding_lock interned_str;
    };
    unsafe { self.write_lock.unlock() };
    ret
  }
}

struct Page {
  // safety: `next_page` may *only* be read or written to while `write_lock` is
  // held.
  // TODO store this pointer in the memory to avoid the extra layer of
  // indirection
  next_page: MaybeUninit<OnceCell<&'static Page>>,
  // A page of memory containing the bytes of our interned data. The size of
  // the page is dynamic and determined by the len of the slice.
  mem: UnsafeCell<&'static mut [u8]>,
}

impl Page {
  /// A Page has a size some multiple of this value
  const DEFAULT_CAPACITY: usize = 1024;

  /// Create a new page with at least min_capacity bytes available
  #[inline]
  fn with_min_capacity(min_capacity: usize) -> &'static Self {
    // round min_capacity up to nearest integer multiple of DEFAULT_CAPACITY.
    let capacity = ((min_capacity / Self::DEFAULT_CAPACITY)
      * Self::DEFAULT_CAPACITY)
      + (usize::min(1, min_capacity % Self::DEFAULT_CAPACITY)
        * Self::DEFAULT_CAPACITY);

    let mem = vec![0; capacity];
    let mem = Box::leak(mem.into_boxed_slice());

    Box::leak(Box::new(Page {
      mem: UnsafeCell::new(mem),
      next_page: MaybeUninit::new(OnceCell::new()),
    }))
  }

  /// Panics if the `next_page` field is already occupied
  ///
  /// # Safety
  ///
  /// - must only be called while holding the `write_lock`
  #[inline]
  unsafe fn extend_with_new_page(&self, min_capacity: usize) {
    // safety: `next_page` will be initialised if the write_lock is held
    let next_page = unsafe { self.next_page.assume_init_ref() };
    if next_page.get().is_some() {
      panic!("The next_page already exists");
    }
    #[allow(clippy::needless_borrow)] // this lint is wrong here??
    let len = unsafe { &*self.mem.get() }.len();
    // next page should be double the size of the current page (at least)
    let min_capacity = usize::max(len * 2, min_capacity);
    let _ = next_page.set(Page::with_min_capacity(min_capacity));
  }
}
