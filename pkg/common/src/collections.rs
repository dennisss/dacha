use core::fmt::Debug;
use core::iter::Iterator;
use core::marker::PhantomData;
use core::mem::zeroed;
use core::mem::MaybeUninit;
use core::ops::{Deref, DerefMut};

use crate::const_default::ConstDefault;

#[derive(Clone, PartialEq)]
pub struct FixedString<A> {
    data: A,
    length: usize,
}

impl<A: AsRef<[u8]> + AsMut<[u8]>> FixedString<A> {
    pub const fn new(data: A) -> Self {
        Self { data, length: 0 }
    }

    pub fn push(&mut self, c: char) {
        let remaining = &mut self.data.as_mut()[self.length..];
        self.length += c.encode_utf8(remaining).len();
    }

    pub fn push_str(&mut self, s: &str) {
        let remaining = &mut self.data.as_mut()[self.length..];
        remaining[0..s.len()].copy_from_slice(s.as_bytes());
        self.length += s.len();
    }
}

impl<A: ConstDefault> Default for FixedString<A> {
    fn default() -> Self {
        Self {
            data: A::DEFAULT,
            length: 0,
        }
    }
}

// impl<A: AsRef<[u8]> + AsMut<[u8]> + Default> From<&str> for FixedString<A> {
//     fn from(v: &str) -> Self {
//         let mut inst = Self::new(A::default());
//         inst.push_str(v);
//         inst
//     }
// }

impl<A: AsRef<[u8]> + AsMut<[u8]> + ConstDefault> From<&str> for FixedString<A> {
    fn from(s: &str) -> Self {
        let mut inst = Self::DEFAULT;
        inst.push_str(s);
        inst
    }
}

impl<A: AsRef<[u8]>> AsRef<[u8]> for FixedString<A> {
    fn as_ref(&self) -> &[u8] {
        &self.data.as_ref()[0..self.length]
    }
}

impl<A: AsRef<[u8]>> AsRef<str> for FixedString<A> {
    fn as_ref(&self) -> &str {
        // All operations we implement are valid UTF-8 mutations so the underlying
        // storage should always contain valid UTF-8 data.
        unsafe { core::str::from_utf8_unchecked(AsRef::<[u8]>::as_ref(self)) }
    }
}

impl<A: AsRef<[u8]> + AsMut<[u8]>> Deref for FixedString<A> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl<A: AsRef<[u8]> + AsMut<[u8]> + ConstDefault> ConstDefault for FixedString<A> {
    const DEFAULT: Self = Self::new(A::DEFAULT);
}

impl<A: AsRef<[u8]>> Debug for FixedString<A> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s: &str = self.as_ref();
        s.fmt(f)
    }
}
