use alloc::{
    borrow::ToOwned,
    string::{String, ToString},
};
use core::{borrow::Borrow, fmt::Debug, ops::Deref};

use common::errors::*;

/// TODO: Support windows.
const SEGMENT_DELIMITER: char = '/';

/*
Differences between this and the 'std' Path
- All methods here are purely string manipulations.

Some notes on normalization:
- "" == "."
- "/../" == "/"
- "hello/" == "hello"



- Linux technically allows any byte string to be used as a path with the '/' byte reserved as the directory delimiter.
- We assume that all paths involed are interpretable as UTF-8
- '/' should never appear as a byte of a character code for any character other than '/' (as UTF-8 encoded all code points >=128 using bytes with the top bit set).
    - So we should never interpret the path incorrectly, but it is possible that we may reject some paths.

- To be useful, linux does effectively require paths to not contain the null byte to be usable as c strings, but we don't check for that until the string is converted to a cstr

*/

// TODO: Implement custom eq?
#[derive(Clone, PartialOrd, Ord, PartialEq, Eq)]
pub struct LocalPathBuf {
    inner: String,
}

impl LocalPathBuf {
    pub fn push<P: AsRef<LocalPath>>(&mut self, other: P) {
        self.push_impl(other.as_ref())
    }

    fn push_impl(&mut self, other: &LocalPath) {
        if other.is_absolute() {
            self.inner.truncate(0);
            self.inner.push_str(other.as_str());
            return;
        }

        if !self.inner.ends_with(SEGMENT_DELIMITER) {
            self.inner.push(SEGMENT_DELIMITER);
        }

        self.inner.push_str(other.as_str());
    }

    pub fn pop(&mut self) -> bool {
        if let Some(parent) = self.parent() {
            // TODO: Check if this is a good behavior
            // if parent.as_str() == "." {
            //     self.inner.truncate(0);
            //     self.inner.push('.');
            //     return true;
            // }

            self.inner.truncate(parent.as_str().len());
            true
        } else {
            false
        }
    }

    /// NOTE: We assume that 'name' doesn't contain any '/' and does not equals
    /// '..' or '.'.
    pub fn set_file_name(&mut self, name: &str) {
        if self.file_name().is_some() {
            self.pop();
        }

        self.push(name);
    }

    pub fn set_extension(&mut self, extension: &str) {
        let new_name = format!("{}.{}", self.file_stem().unwrap(), extension);
        self.set_file_name(&new_name)
    }

    pub fn as_path(&self) -> &LocalPath {
        self.as_ref()
    }

    /*
    pub fn normalize(&mut self) {
        //

    }
    */
}

impl Debug for LocalPathBuf {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        AsRef::<LocalPath>::as_ref(self).fmt(f)
    }
}

impl common::args::ArgType for LocalPathBuf {
    fn parse_raw_arg(raw_arg: common::args::RawArgValue) -> Result<Self> {
        match raw_arg {
            common::args::RawArgValue::Bool(_) => Err(err_msg("Expected string, got bool")),
            common::args::RawArgValue::String(s) => Ok(LocalPathBuf::from(s)),
        }
    }
}

impl<S: Into<String>> From<S> for LocalPathBuf {
    fn from(inner: S) -> Self {
        Self {
            inner: inner.into(),
        }
    }
}

impl AsRef<LocalPath> for LocalPathBuf {
    fn as_ref(&self) -> &LocalPath {
        LocalPath::new(&self.inner)
    }
}

impl AsRef<std::path::Path> for LocalPathBuf {
    fn as_ref(&self) -> &std::path::Path {
        self.inner.as_ref()
    }
}

impl Borrow<LocalPath> for LocalPathBuf {
    fn borrow(&self) -> &LocalPath {
        self.as_ref()
    }
}

impl Deref for LocalPathBuf {
    type Target = LocalPath;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
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

    pub fn join<P: AsRef<LocalPath>>(&self, other: P) -> LocalPathBuf {
        let mut p = self.to_owned();
        p.push(other);
        p
    }

    pub fn segments(&self) -> impl Iterator<Item = LocalPathSegment> {
        LocalPathSegmentIterator {
            started: false,
            remaining: self.as_str(),
        }
    }

    pub fn normalized(&self) -> LocalPathBuf {
        let mut out = LocalPathBuf::from(".");

        for segment in self.segments() {
            match segment {
                LocalPathSegment::Root => out.push("/"),
                LocalPathSegment::CurrentDir => {}
                LocalPathSegment::ParentDir => {
                    out.pop();
                }
                LocalPathSegment::File(p) => out.push(p),
            }
        }

        out
    }

    /// NOTE: It only makes sense to call this on a normalized path.
    pub fn starts_with<P: AsRef<LocalPath>>(&self, other: P) -> bool {
        self.starts_with_impl(other.as_ref())
    }

    fn starts_with_impl(&self, other: &LocalPath) -> bool {
        self.strip_prefix(other).is_some()
    }

    pub fn strip_prefix<P: AsRef<LocalPath>>(&self, other: P) -> Option<&LocalPath> {
        self.strip_prefix_impl(other.as_ref())
    }

    // /a/b/c
    // /a

    fn strip_prefix_impl(&self, other: &LocalPath) -> Option<&LocalPath> {
        let other = other.as_str().trim_end_matches(SEGMENT_DELIMITER);

        let rest = match self.as_str().strip_prefix(other) {
            Some(v) => v,
            None => return None,
        };

        if rest.is_empty() {
            return Some(LocalPath::new(rest));
        }

        if let Some(rest) = rest.strip_prefix(SEGMENT_DELIMITER) {
            return Some(LocalPath::new(rest));
        }

        None
    }

    /*
    First remove ending "/"

    If the path ends with ".." or ".", then

    Return values:
    - (None, None)

    */

    fn split_last_segment(&self) -> (Option<&str>, LocalPathSegment) {
        // Only applies if the name is not "////"
        let mut s = self.as_str().trim_end_matches(SEGMENT_DELIMITER);
        if s.is_empty() && !self.as_str().is_empty() {
            s = "/";
        }

        if s == "/" {
            return (None, LocalPathSegment::Root);
        }

        // if s == "." {
        //     return (None, LocalPathSegment::CurrentDir);
        // }

        let (parent, last) = match s.rfind(SEGMENT_DELIMITER) {
            Some(pos) => {
                let (s, e) = s.split_at(pos + 1);
                (Some(s), e)
            }
            None => (None, s),
        };

        let segment = {
            if last.is_empty() || last == "." {
                LocalPathSegment::CurrentDir
            } else if last == ".." {
                LocalPathSegment::ParentDir
            } else {
                LocalPathSegment::File(last)
            }
        };

        (parent, segment)
    }

    pub fn parent(&self) -> Option<&LocalPath> {
        let (p, _) = self.split_last_segment();
        p.map(|p| LocalPath::new(p))
    }

    /// Splits a file name into a stem and an extension
    /// (we assume the file name is not empty).
    fn split_file_name(name: &str) -> (&str, Option<&str>) {
        match name.rsplit_once(".") {
            Some((start, rest)) => {
                if start.is_empty() {
                    // Name starts with a '.' and has no other '.' s in it.
                    return (name, None);
                }

                (start, Some(rest))
            }
            None => (name, None),
        }
    }

    // TODO: Filename of "hello/" should be "hello"
    pub fn file_name(&self) -> Option<&str> {
        let (_, last_segment) = self.split_last_segment();

        if let LocalPathSegment::File(name) = last_segment {
            return Some(name);
        } else {
            None
        }
    }

    pub fn file_stem(&self) -> Option<&str> {
        let file_name = match self.file_name() {
            Some(v) => v,
            None => return None,
        };

        let (stem, _) = Self::split_file_name(file_name);
        Some(stem)
    }

    pub fn extension(&self) -> Option<&str> {
        let file_name = match self.file_name() {
            Some(v) => v,
            None => return None,
        };

        let (_, ext) = Self::split_file_name(file_name);
        ext
    }
}

impl<P: AsRef<LocalPath>> PartialEq<P> for LocalPath {
    fn eq(&self, other: &P) -> bool {
        self.as_str() == other.as_ref().as_str()
    }
}

impl PartialEq<LocalPath> for LocalPath {
    fn eq(&self, other: &LocalPath) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Debug for LocalPath {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "\"{}\"", self.as_str())
    }
}

impl ToOwned for LocalPath {
    type Owned = LocalPathBuf;

    fn to_owned(&self) -> Self::Owned {
        LocalPathBuf::from(self.inner.to_string())
    }
}

impl AsRef<std::path::Path> for LocalPath {
    fn as_ref(&self) -> &std::path::Path {
        std::path::Path::new(self.as_str())
    }
}

impl AsRef<LocalPath> for LocalPath {
    fn as_ref(&self) -> &LocalPath {
        self
    }
}

impl AsRef<LocalPath> for str {
    fn as_ref(&self) -> &LocalPath {
        LocalPath::new(self)
    }
}

impl AsRef<LocalPath> for String {
    fn as_ref(&self) -> &LocalPath {
        LocalPath::new(self.as_str())
    }
}

#[derive(PartialEq, Eq, Debug)]
pub enum LocalPathSegment<'a> {
    Root,
    CurrentDir,
    ParentDir,
    File(&'a str),
}

impl<'a> LocalPathSegment<'a> {
    pub fn as_str(&self) -> &str {
        match self {
            LocalPathSegment::Root => "/",
            LocalPathSegment::CurrentDir => ".",
            LocalPathSegment::ParentDir => "..",
            LocalPathSegment::File(v) => v,
        }
    }
}

struct LocalPathSegmentIterator<'a> {
    remaining: &'a str,
    started: bool,
}

impl<'a> Iterator for LocalPathSegmentIterator<'a> {
    type Item = LocalPathSegment<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.started {
            self.started = true;

            if let Some(r) = self.remaining.strip_suffix(SEGMENT_DELIMITER) {
                self.remaining = r;
                return Some(LocalPathSegment::Root);
            }
        }

        if self.remaining.is_empty() {
            return None;
        }

        let (cur, rest) = self
            .remaining
            .split_once(SEGMENT_DELIMITER)
            .unwrap_or_else(|| (self.remaining, ""));
        self.remaining = rest;

        if cur == "." {
            Some(LocalPathSegment::CurrentDir)
        } else if cur == ".." {
            Some(LocalPathSegment::ParentDir)
        } else {
            Some(LocalPathSegment::File(cur))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_functions() {
        assert_eq!(LocalPath::new("/").join("hello").as_str(), "/hello");
        assert_eq!(LocalPath::new("/var").join("/opt").as_str(), "/opt");
        assert_eq!(
            LocalPath::new("relative/path")
                .join("to/something")
                .as_str(),
            "relative/path/to/something"
        );
        assert_eq!(LocalPath::new("/var/").join("run").as_str(), "/var/run");

        assert_eq!(
            LocalPath::new("/a/b/c")
                .strip_prefix("/a")
                .unwrap()
                .as_str(),
            "b/c"
        );

        let mut p = LocalPath::new("/var/run/something.txt").to_owned();
        assert_eq!(p.extension(), Some("txt"));

        p.set_extension("rs");
        assert_eq!(p.as_str(), "/var/run/something.rs");
        assert_eq!(p.file_name(), Some("something.rs"));

        assert!(p.pop());
        assert_eq!(p.as_str(), "/var/run/");
        assert_eq!(p.file_name(), Some("run"));

        assert_eq!(
            LocalPath::new("/../../hello/world/./jello/apples/../file/")
                .normalized()
                .as_str(),
            "/hello/world/jello/file"
        );
        assert_eq!(LocalPath::new("/file/").normalized().as_str(), "/file");
        assert_eq!(LocalPath::new("file/").normalized().as_str(), "file");
        assert_eq!(LocalPath::new("./file/").normalized().as_str(), "file");
        assert_eq!(LocalPath::new("").normalized().as_str(), ".");
        assert_eq!(LocalPath::new("/../../").normalized().as_str(), "/");

        // TODO: This is slightly different than doing .pop() right now.
        // assert_eq!(LocalPath::new("hello").parent(),
        // Some(LocalPath::new(".")));
    }
}
