use ::core::ptr::NonNull;

/// Adds `into_non_null` and `from_non_null` equivalents to
/// [`Box::into_raw`] and [`Box::from_raw`]
pub trait BoxNonNull<T> {
  /// Consumes the [`Box`], returning a [`NonNull`].
  ///
  /// The pointer will be properly aligned.
  ///
  /// After calling this function, the caller is responsible for the memory
  /// previously managed by the [`Box`]. In particular, the caller should
  /// properly destroy `T` and release the memory, taking into account the
  /// memory layout used by [`Box`]. The easiest way to do this is to convert
  /// the [`NonNull`] back into a [`Box`] with the
  /// [`Box::from_non_null`][Self::from_non_null] function, allowing the
  /// [`Box`] destructor to perform the cleanup.
  ///
  /// ## Examples
  ///
  /// Converting the [`NonNull`] pointer back into a [`Box`] with
  /// [`Box::from_non_null`][Self::from_non_null] for automatic cleanup:
  ///
  /// ```rust
  /// let x = Box::new(5);
  /// let ptr = Box::into_raw(x);
  /// let x = unsafe { Box::from_raw(ptr) };
  /// ```
  #[inline]
  fn into_non_null(b: Box<T>) -> NonNull<T> {
    let ptr = Box::into_raw(b);
    unsafe { NonNull::new_unchecked(ptr) }
  }

  /// Constructs a box from a [`NonNull`] pointer.
  ///
  /// After calling this function, the [`NonNull`] pointer is owned by the
  /// resulting [`Box`]. Specifically, the Box destructor will call the
  /// destructor of `T` and free the allocated memory. For this to be safe, the
  /// memory must have been allocated in accordance with the memory layout used
  /// by [`Box`].
  ///
  /// ## Safety
  ///
  /// This function is unsafe because improper use may lead to memory problems.
  /// For example, a double-free may occur if the function is called twice on
  /// the same [`NonNull`] pointer.
  ///
  /// ## Examples
  ///
  /// Recreate a [`Box`] which was previously converted to a [`NonNull`]
  /// pointer using [`Box::into_non_null`][Self::into_non_null]:
  ///
  /// ```rust
  /// let x = Box::new(5);
  /// let ptr = Box::into_raw(x);
  /// let x = unsafe { Box::from_raw(ptr) };
  /// ```
  #[inline]
  unsafe fn from_non_null(n: NonNull<T>) -> Box<T> {
    let ptr = n.as_ptr();
    unsafe { Box::from_raw(ptr) }
  }
}

impl<T> BoxNonNull<T> for Box<T> {}
