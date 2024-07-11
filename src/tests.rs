use {
  super::*,
  ::core::{ffi::CStr, iter},
  ::std::ffi::CString,
};

#[test]
fn eq() {
  let hello_world = intern(&"hello, world".to_owned());
  let banana = intern("banana");
  let _ = intern("apple");
  let hello_world2 = intern("hello, world");

  assert_eq!(
    hello_world, hello_world2,
    "Interning strings with the same value should be equivalent"
  );
  assert_ne!(
    hello_world, banana,
    "Interning strings with different values should not be equivalent"
  );
}

#[test]
fn constructors() {
  let string: String = String::from("hello");
  let ref_str: &str = &string;

  let from_ref_str: IStr = IStr::from(ref_str);
  let from_ref_string: IStr = IStr::from(&string);
  let from_string: IStr = IStr::from(string);

  let c_string: CString = CString::new("hello").unwrap();
  let ref_c_str: &CStr = &c_string;

  let try_from_ref_c_str: IStr = IStr::try_from(ref_c_str).unwrap();
  let try_from_ref_c_string: IStr = IStr::try_from(&c_string).unwrap();
  let try_from_c_string: IStr = IStr::try_from(c_string).unwrap();

  let istr = IStr::new("hello");

  assert_eq!(istr, from_ref_str);
  assert_eq!(istr, from_ref_string);
  assert_eq!(istr, from_string);

  assert_eq!(istr, try_from_ref_c_str);
  assert_eq!(istr, try_from_ref_c_string);
  assert_eq!(istr, try_from_c_string);
}

#[test]
fn partial_eq() {
  let hello_str: &str = "hello";
  let hello_string = String::from(hello_str);
  let hello_cstring = CString::new(hello_str).unwrap();
  let hello_cstr: &CStr = &hello_cstring;

  let hello_istr = IStr::new(hello_str);

  // note we don't intend to support double references or references to IStrs
  // since they should be deref'ed

  // istr
  assert_eq!(hello_istr, hello_istr);
  assert_eq!(hello_istr, *&hello_istr);
  assert_eq!(*&hello_istr, hello_istr);
  assert_eq!(&hello_istr, &hello_istr);

  // str
  assert_eq!(hello_istr, hello_str);
  assert_eq!(hello_istr, *&hello_str);
  assert_eq!(*&hello_istr, hello_str);
  assert_eq!(&hello_istr, &hello_str);

  assert_eq!(hello_str, hello_istr);
  assert_eq!(hello_str, *&hello_istr);
  assert_eq!(*&hello_str, hello_istr);
  assert_eq!(&hello_str, &hello_istr);

  // string
  assert_eq!(hello_istr, hello_string);
  assert_eq!(hello_istr, &hello_string);
  assert_eq!(*&hello_istr, hello_string);
  assert_eq!(&hello_istr, &hello_string);

  assert_eq!(hello_string, hello_istr);
  assert_eq!(hello_string, *&hello_istr);
  assert_eq!(&hello_string, hello_istr);
  assert_eq!(&hello_string, &hello_istr);

  // cstr
  assert_eq!(hello_istr, hello_cstr);
  assert_eq!(hello_istr, *&hello_cstr);
  assert_eq!(*&hello_istr, hello_cstr);
  assert_eq!(&hello_istr, &hello_cstr);

  assert_eq!(hello_cstr, hello_istr);
  assert_eq!(hello_cstr, *&hello_istr);
  assert_eq!(*&hello_cstr, hello_istr);
  assert_eq!(&hello_cstr, &hello_istr);

  // cstring
  assert_eq!(hello_istr, hello_cstring);
  assert_eq!(hello_istr, &hello_cstring);
  assert_eq!(*&hello_istr, hello_cstring);
  assert_eq!(&hello_istr, &hello_cstring);

  assert_eq!(hello_cstring, hello_istr);
  assert_eq!(hello_cstring, *&hello_istr);
  assert_eq!(&hello_cstring, hello_istr);
  assert_eq!(&hello_cstring, &hello_istr);
}

#[test]
fn long_strings() {
  let hello = intern("hello");
  let e = intern(&String::from_iter(iter::repeat('E').take(4000)));
  let world = intern("world");
  let seven = intern(&String::from_iter(iter::repeat('7').take(7777)));

  let hello2 = intern("hello");
  let e2 = intern(&String::from_iter(iter::repeat('E').take(4000)));
  let world2 = intern("world");
  let seven2 = intern(&String::from_iter(iter::repeat('7').take(7777)));

  assert_eq!(hello, hello2,);
  assert_eq!(
    e, e2,
    "Interning long strings with the same value should be Eq"
  );
  assert_eq!(world, world2,);
  assert_eq!(
    seven, seven2,
    "Interning long strings with the same value should be Eq"
  );
}

#[test]
fn concurrency() {
  use ::std::{collections::HashMap, thread};

  let mut a = HashMap::new();
  let mut b = HashMap::new();
  let mut c = HashMap::new();
  let mut d = HashMap::new();

  const COUNT: usize = 1_000;

  thread::scope(|s| {
    s.spawn(|| {
      for i in 0..COUNT {
        a.insert(i, intern(&format!("{i}")));
      }
    });
    s.spawn(|| {
      for i in 0..COUNT {
        b.insert(i, intern(&format!("{i}")));
      }
    });
    s.spawn(|| {
      for i in 0..COUNT {
        c.insert(i, intern(&format!("{i}")));
      }
    });
    s.spawn(|| {
      for i in 0..COUNT {
        d.insert(i, intern(&format!("{i}")));
      }
    });
  });

  let mut results_a_b = Vec::new();
  let mut results_a_c = Vec::new();
  let mut results_a_d = Vec::new();

  for i in 0..COUNT {
    // note: these equality checks are implemented as pointer comparisons, so
    // we're checking if all the duplicates really are identical IStrs
    results_a_b.push(a[&i] == b[&i]);
    results_a_c.push(a[&i] == c[&i]);
    results_a_d.push(a[&i] == d[&i]);
  }

  assert_eq!(
    &results_a_b, &[true; COUNT],
    "Interning the same strings from different threads should be Eq"
  );
  assert_eq!(
    &results_a_c, &[true; COUNT],
    "Interning the same strings from different threads should be Eq"
  );
  assert_eq!(
    &results_a_d, &[true; COUNT],
    "Interning the same strings from different threads should be Eq"
  );
}

#[test]
fn wyhash() {
  use crate::interner::WYHASH_SEED;

  let hello_world = IStr::new("hello, world!");

  assert_eq!(
    hello_world.wyhash(),
    ::wyhash::wyhash("hello, world!".as_bytes(), WYHASH_SEED)
  )
}

#[test]
fn slice_index() {
  let hello_world = IStr::new("hello, world!");
  let hello: &str = &hello_world[..5];
  assert_eq!(hello, "hello");
}

#[test]
fn hash() {
  use ::std::collections::HashMap;
  let mut map: HashMap<IStr, u32> = HashMap::new();

  map.insert(IStr::new("key1234"), 1234);
  assert_eq!(Some(&1234), map.get(&IStr::new("key1234")));

  // check the Borrow<str> impl
  assert_eq!(Some(&1234), map.get("key1234"));
}

// can't run this test without disabling the others, since the pool is shared
// #[test]
fn _collect() {
  use ::std::collections::HashSet;

  const COUNT: usize = 100;

  let mut set = HashSet::new();
  for i in 0..COUNT {
    set.insert(intern(&format!("{i}")));
  }

  let mut istrs = collect_interned_strings::<Vec<_>>();
  istrs.sort();

  let mut set = set.into_iter().collect::<Vec<_>>();
  set.sort();

  assert_eq!(istrs, set);
}
