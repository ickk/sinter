#![doc = include_str!("../README.md")]

mod ext;
mod internal;
mod istr;
#[cfg(any(test, doctest))]
mod tests;

pub use istr::{collect_interned_strings, get_interned, intern, IStr};
