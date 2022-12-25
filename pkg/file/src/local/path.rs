use alloc::string::String;
use core::ops::Deref;

/// TODO: Support windows.
const SEGMENT_DELIMITER: char = '/';

/*
- Linux technically allows any byte string to be used as a path with the '/' byte reserved as the directory delimiter.
- We assume that all paths involed are interpretable as UTF-8
- '/' should never appear as a byte of a character code for any character other than '/' (as UTF-8 encoded all code points >=128 using bytes with the top bit set).
    - So we should never interpret the path incorrectly, but it is possible that we may reject some paths.

- To be useful, linux does effectively require paths to not contain the null byte to be usable as c strings, but we don't check for that until the string is converted to a cstr

*/

pub struct LocalPathBuf {
    inner: String,
}

impl Deref for LocalPathBuf {
    type Target = LocalPath;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl AsRef<LocalPath> for LocalPathBuf {
    fn as_ref(&self) -> &LocalPath {
        LocalPath::new(&self.inner)
    }
}

pub struct LocalPath {
    inner: str,
}

impl LocalPath {
    pub fn new<S: ?Sized + AsRef<str>>(value: &S) -> &Self {
        unsafe { core::mem::transmute(value.as_ref()) }
    }

    pub fn as_str(&self) -> &str {
        &self.inner
    }

    pub fn is_absolute(&self) -> bool {
        self.inner.starts_with(SEGMENT_DELIMITER)
    }
}

impl AsRef<LocalPath> for str {
    fn as_ref(&self) -> &LocalPath {
        LocalPath::new(self)
    }
}
