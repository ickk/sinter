`sinter`
==========
[crates.io](https://crates.io/crates/sinter) |
[docs.rs](https://docs.rs/sinter) |
[github](https://github.com/ickk/sinter)

An easy to use & fast global interning pool.

Interned strings are stored contiguously in memory, which may help with memory
locality or fragmentation. Additional pages of memory for the interner are
allocated as required, doubling in size with each successive page - amortising
the cost of the underlying allocations.

Calling [`intern`] on a string that has already previously been interned is
fast & lockless, though still potentially more expensive than holding onto an
[`IStr`] you already have.

In the worst case a call to [`intern`] can be relatively expensive, since if
the string doesn't already exist then some synchronisation with other threads
is required, and the operation may also require allocating a new memory page
for the pool.

`IStr`
------

Zero-cost conversion to `&'static str` or `&'static CStr`:
```rust
# use sinter::IStr;
# use ::core::ffi::CStr;
let istr = IStr::new("hello, sinter!");
let s: &'static str = istr.as_str();
let cstr: &'static CStr = istr.as_c_str();
```

[`IStr`] Derefs to `&str`:
```rust
# use sinter::IStr;
# use ::core::ffi::CStr;
let istr = IStr::new("hello, sinter!");
let s: &str = &*istr;
```

An [`IStr`] can be compared to another `IStr` extremely cheaply; under the hood
[`Eq`] is implemented by a single pointer comparison:
```rust
# use sinter::intern;
# use ::core::ffi::CStr;
let a = intern("aaa");
let a2 = intern("aaa");
let b = intern("bbb");

assert!(a == a2);
assert!(a != b);
```

Or you can compare to a regular `&str`:
```rust
# use sinter::IStr;
assert!(IStr::new("sinter") == "sinter");
```

Flexible to construct:
```rust
# use sinter::{intern, IStr};
# use ::std::ffi::{CStr, CString};
let a = intern("a");
let b = IStr::new("b");
let c = IStr::from("c");
let d = IStr::from(String::from("d"));
let e: IStr = "e".into();
let f = IStr::try_from(CString::new("f").unwrap()).unwrap();
let g = IStr::try_from(CString::new("g").unwrap().as_c_str()).unwrap();
# assert_eq!(
#   [a, b, c, d, e, f, g],
#   [
#     intern("a"),
#     intern("b"),
#     intern("c"),
#     intern("d"),
#     intern("e"),
#     intern("f"),
#     intern("g"),
#   ],
# );
```

Find out if a given string has already been interned with [`get_interned`].
This will always be fast/lockless and returns the [`IStr`] if found:
```rust
# use sinter::{get_interned, intern};
intern("exists");
assert!(get_interned("exists").is_some());
assert!(get_interned("doesn't exist").is_none());
```

The [`::core::ops::Deref`] implementation gives you all the
useful `&str` methods & operations, such as subslicing:
```rust
# use sinter::IStr;
let hello_world = IStr::new("hello, world!");
let world: &str = &hello_world[7..];
assert_eq!(world, "world!");
```

The [`::core::borrow::Borrow<str>`] implementation lets you create `HashMap`s
with `IStr` keys, and then ergonomically lookup values with `&str`:
```rust
# use sinter::IStr;
# use ::std::collections::HashMap;
let mut map: HashMap<IStr, f32> = HashMap::new();
map.insert(IStr::new("e"), 2.718);
let val = map.get("e");
# assert_eq!(val, Some(&2.718));
```

Architecture
------------

Internally, an `Interner` data structure manages the pool of interned strings.

When adding a new string to the pool, the Interner acquires a lock on one half
of the pool. This could be a somewhat slow operation if there is a lot of
contention with other threads (although this should normally be very unlikely).

On the other hand, the Interner uses lockless concurrency primitives to enable
readers (callers to `intern` that do not require allocating a new string, and
instead can fetch an existing `IStr` instance) to avoid locking entirely,
allowing superfluous calls to `intern` to still be very fast.

The concurrency scheme is as follows:

1. We maintain a linked-list of memory pages where the strings themselves are
   stored. New strings are appended strictly to the tail of the last memory
   page, and new pages are allocated as needed. This means all existing `IStr`s
   have stable static memory locations and data.

2. We maintain a pair of redundant hash tables mapping a string's hash to the
   `IStr` (the pointer to the string data in the memory page), facilitating
   fast lookup for already interned strings. The tables are atomically swapped
   by the writer, allowing readers to safely get new updates without locking.

   When a thread wants to inspect the "readable table" they increment an atomic
   counter. This counter is incremented again when the reader is finished. This
   allows a writer to reliably wait on lingering reads after the atomic table
   swap.

   If each thread's counter is even, then the writer knows they are not reading
   at all. If the thread's counter is odd, then the writer waits for the
   counter to increment at least once. Note, waiting for a single increment is
   sufficient and should be fairly quick (as reads are quick). After any
   increment the writer can be sure the reader will fetch the new table's
   pointer before starting a new read.

3. When a thread terminates it calls the destructor for the `LocalKey` which
   contains a pointer to our epoch atomic-counter. In this destructor we set
   the value of the epoch to a special value to mark this thread as dead. Later
   when some other code is holding the write_lock on the interner, it checks
   the list of epochs to see if any threads are dead, and then frees the memory
   holding the atomic and removes that epoch from Interner's list.

   This solves the small memory leak that might occur if the user keeps
   spawning lots of temporary threads. It doesn't require that the LocalKey
   destructor waits until it can get the write_lock, and it avoids accumulating
   dangling pointers in the Interner datastructure.

License
-------

This crate is licensed under any of the
[Apache license, Version 2.0](./LICENSE-APACHE),
or the
[MIT license](./LICENSE-MIT),
or the
[Zlib license](./LICENSE-ZLIB)
at your option.

Unless explicitly stated otherwise, any contributions you intentionally submit
for inclusion in this work shall be licensed accordingly.
