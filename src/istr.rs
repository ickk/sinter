use {
  ::core::{
    borrow::Borrow,
    convert::AsRef,
    ffi::CStr,
    fmt::{self, Debug, Display},
    hash::Hash,
    ops::Deref,
  },
  ::std::ffi::CString,
};

/// An Interned string
#[derive(Eq, Copy, Clone, PartialOrd, Ord)]
pub struct IStr(pub(super) &'static str);

// # constructors

macro_rules! intern_doc {() => {
r"Intern a new string, or return the extant [`IStr`] is one exists

This operation may be slow, depending on whether the string has been previously
interned."
};}
#[doc = intern_doc!()]
#[inline]
pub fn intern(s: &str) -> IStr {
  crate::internal::THE_INTERNER.intern(s)
}

/// Locklessly find an extant [`IStr`] corresponding to the string given, if
/// one exists
///
/// Call this to find out if a string has already been interned,
/// without newly interning it if not.
#[inline]
pub fn get_interned(s: &str) -> Option<IStr> {
  crate::internal::THE_INTERNER.get_interned(s)
}

/// Create a collection of all the currently interned strings
///
/// The order of the items in the collection may not be stable.
///
/// ```rust
/// # use sinter::{IStr, collect_interned_strings};
/// let istrs: Vec<IStr> = collect_interned_strings();
/// ```
#[inline]
pub fn collect_interned_strings<B>() -> B
where
  B: ::core::iter::FromIterator<IStr>,
{
  crate::internal::THE_INTERNER.collect_interned_strings()
}

impl IStr {
  #[doc = intern_doc!()]
  #[inline]
  pub fn new(s: &str) -> Self {
    intern(s)
  }
}

impl From<&'_ str> for IStr {
  #[doc = intern_doc!()]
  #[inline]
  fn from(s: &str) -> Self {
    intern(s)
  }
}

impl From<String> for IStr {
  #[doc = intern_doc!()]
  #[inline]
  fn from(s: String) -> Self {
    intern(&s)
  }
}

impl TryFrom<&'_ CStr> for IStr {
  type Error = ::core::str::Utf8Error;

  #[doc = intern_doc!()]
  #[inline]
  fn try_from(c: &'_ CStr) -> Result<Self, Self::Error> {
    let s = c.to_str()?;
    Ok(intern(s))
  }
}

impl TryFrom<CString> for IStr {
  type Error = ::core::str::Utf8Error;

  #[doc = intern_doc!()]
  #[inline]
  fn try_from(c: CString) -> Result<Self, Self::Error> {
    let s = c.to_str()?;
    Ok(intern(s))
  }
}

// # reference types & conversion

impl Deref for IStr {
  type Target = str;

  #[inline]
  fn deref(&self) -> &str {
    self.0
  }
}

impl AsRef<str> for IStr {
  #[inline]
  fn as_ref(&self) -> &str {
    self.0
  }
}

impl AsRef<CStr> for IStr {
  #[inline]
  fn as_ref(&self) -> &CStr {
    self.as_c_str()
  }
}

impl Borrow<str> for IStr {
  #[inline]
  fn borrow(&self) -> &'static str {
    self.0
  }
}

impl IStr {
  /// get the underlying `&str`
  #[inline]
  pub fn as_str(&self) -> &'static str {
    self.0
  }

  /// zero-cost conversion to a null terminated [`CStr`]
  #[inline]
  pub fn as_c_str(&self) -> &'static CStr {
    let ptr = self.0.as_ptr();
    // safety: The Interner always leaves a trailing null byte
    unsafe { CStr::from_ptr(ptr as _) }
  }
}

impl From<IStr> for &'static str {
  #[inline]
  fn from(i: IStr) -> &'static str {
    i.as_str()
  }
}

impl From<IStr> for &'static CStr {
  #[inline]
  fn from(i: IStr) -> &'static CStr {
    i.as_c_str()
  }
}

impl From<IStr> for String {
  #[inline]
  fn from(s: IStr) -> String {
    s.0.to_owned()
  }
}

impl From<IStr> for CString {
  #[inline]
  fn from(s: IStr) -> CString {
    s.as_c_str().to_owned()
  }
}

impl IStr {
  /// Create a new [`CString`] from the value
  #[inline]
  pub fn to_c_string(&self) -> CString {
    self.as_c_str().to_owned()
  }
}

// note: `Display` gives us `to_string()` by way of `ToString`
impl Display for IStr {
  #[inline]
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(self.0)
  }
}

impl Debug for IStr {
  #[inline]
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_fmt(format_args!("IStr(\"{}\")", self.0))
  }
}

// # equality

// istr
impl PartialEq for IStr {
  /// fast [`IStr`] comparison (pointer equality test)
  #[inline]
  fn eq(&self, rhs: &IStr) -> bool {
    // it is sufficient to compare the pointers, because the Interner never
    // produces two distinct [`IStr`]s with the same data, and does not alias
    // strings in the pool.
    ::core::ptr::eq(self.0.as_ptr(), rhs.0.as_ptr())
  }
}

// str
impl PartialEq<&str> for IStr {
  /// full (potentially slow) string comparison
  #[inline]
  fn eq(&self, other: &&str) -> bool {
    self.0 == *other
  }
}
impl PartialEq<IStr> for &str {
  /// full (potentially slow) string comparison
  #[inline]
  fn eq(&self, other: &IStr) -> bool {
    *self == other.0
  }
}

// cstr
impl PartialEq<&CStr> for IStr {
  /// full (potentially slow) string comparison
  #[inline]
  fn eq(&self, other: &&CStr) -> bool {
    self.as_c_str() == *other
  }
}
impl PartialEq<IStr> for &CStr {
  /// full (potentially slow) string comparison
  #[inline]
  fn eq(&self, other: &IStr) -> bool {
    *self == other.as_c_str()
  }
}

// string
impl PartialEq<String> for IStr {
  /// full (potentially slow) string comparison
  #[inline]
  fn eq(&self, other: &String) -> bool {
    self.0 == other
  }
}
impl PartialEq<IStr> for String {
  /// full (potentially slow) string comparison
  #[inline]
  fn eq(&self, other: &IStr) -> bool {
    self == other.0
  }
}
impl PartialEq<&String> for IStr {
  /// full (potentially slow) string comparison
  #[inline]
  fn eq(&self, other: &&String) -> bool {
    self.0 == *other
  }
}
impl PartialEq<IStr> for &String {
  /// full (potentially slow) string comparison
  #[inline]
  fn eq(&self, other: &IStr) -> bool {
    *self == other.0
  }
}

// cstring
impl PartialEq<CString> for IStr {
  /// full (potentially slow) string comparison
  #[inline]
  fn eq(&self, other: &CString) -> bool {
    let o: &CStr = other;
    self.as_c_str() == o
  }
}
impl PartialEq<IStr> for CString {
  /// full (potentially slow) string comparison
  #[inline]
  fn eq(&self, other: &IStr) -> bool {
    let s: &CStr = self;
    s == other.as_c_str()
  }
}
impl PartialEq<&CString> for IStr {
  /// full (potentially slow) string comparison
  #[inline]
  fn eq(&self, other: &&CString) -> bool {
    let o: &CStr = other;
    self.as_c_str() == o
  }
}
impl PartialEq<IStr> for &CString {
  /// full (potentially slow) string comparison
  #[inline]
  fn eq(&self, other: &IStr) -> bool {
    let s: &CStr = self;
    s == other.as_c_str()
  }
}

// # hashing

impl Hash for IStr {
  /// This feeds the underlying &str into the hasher
  #[inline]
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    self.as_str().hash(state);
  }
}

impl IStr {
  /// The [wyhash](https://crates.io/crates/wyhash) value of this string
  ///
  /// This value is cached next to the string by the interner so this method
  /// call is free.
  ///
  /// wyhash is a very cheap non-cryptographic hash function, useful when using
  /// [`IStr`] as a key in a [`hashbrown::HashTable`].
  #[inline]
  pub fn wyhash(&self) -> u64 {
    use crate::internal::SIZE_OF_WYHASH;
    // safety: the Interner caches the u64 wyhash in the 8 bytes preceding the
    // string data
    let hash_array: &[u8; SIZE_OF_WYHASH] = unsafe {
      let hash_ptr = self.0.as_ptr().sub(SIZE_OF_WYHASH);
      &*(hash_ptr as *const [u8; SIZE_OF_WYHASH])
    };
    u64::from_ne_bytes(*hash_array)
  }
}
