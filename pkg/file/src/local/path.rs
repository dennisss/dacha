use alloc::{
    borrow::ToOwned,
    string::{String, ToString},
};
use core::{borrow::Borrow, fmt::Debug, ops::Deref};
use std::ffi::OsStr;

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
#[derive(Clone, PartialOrd, Ord, PartialEq, Eq, Default)]
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

        if !self.inner.is_empty() && !self.inner.ends_with(SEGMENT_DELIMITER) {
            self.inner.push(SEGMENT_DELIMITER);
        }

        self.inner.push_str(other.as_str());
    }

    pub fn pop(&mut self) -> bool {
        if let Some(parent) = self.parent() {
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
}

impl<S: AsRef<str>> PartialEq<S> for LocalPathBuf {
    fn eq(&self, other: &S) -> bool {
        self.as_str() == other.as_ref()
    }
}

impl Debug for LocalPathBuf {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        AsRef::<LocalPath>::as_ref(self).fmt(f)
    }
}

// TODO: Make this into a separate type so the internal behavior isn't hidden?
impl common::args::ArgType for LocalPathBuf {
    fn parse_raw_arg(raw_arg: common::args::RawArgValue) -> Result<Self> {
        let mut s = match raw_arg {
            common::args::RawArgValue::Bool(_) => return Err(err_msg("Expected string, got bool")),
            common::args::RawArgValue::String(s) => s,
        };

        let mut path = if let Some(p) = s.strip_prefix("~") {
            let p = p.trim_start_matches("/");
            let home = std::env::var("HOME")?;
            LocalPath::new(&home).join(p)
        } else {
            LocalPathBuf::from(s)
        };

        if !path.is_absolute() {
            path = crate::current_dir()?.join(path);
        }

        Ok(path.normalized())
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
        LocalPathSegmentIterator::new(self.as_str()).map(|(s, _)| s)
    }

    pub fn rsegments(&self) -> impl Iterator<Item = LocalPathSegment> {
        LocalPathReverseSegmentersIterator::new(self.as_str()).map(|(s, _)| s)
    }

    pub fn normalized(&self) -> LocalPathBuf {
        let mut out = LocalPathBuf::from("");

        let mut in_current_dir = false;
        let mut is_absolute = false;

        for segment in self.segments() {
            match segment {
                LocalPathSegment::Root => {
                    is_absolute = true;
                    out.push("/");
                }
                LocalPathSegment::CurrentDir => {
                    in_current_dir = true;
                }
                LocalPathSegment::ParentDir => {
                    out.pop();

                    if is_absolute && out.as_str().is_empty() {
                        out.push("/");
                    }
                }
                LocalPathSegment::File(p) => out.push(p),
            }
        }

        if in_current_dir && out.as_str().is_empty() {
            out.push(".");
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

    fn strip_prefix_impl(&self, other: &LocalPath) -> Option<&LocalPath> {
        let mut cur_segments = LocalPathSegmentIterator::new(self.as_str());
        let mut other_segments = other.segments();

        let mut final_end_position = 0;
        for expected_segment in other_segments {
            let (segment, end_position) = match cur_segments.next() {
                Some(v) => v,
                None => return None,
            };

            if segment != expected_segment {
                return None;
            }

            final_end_position = end_position;
        }

        Some(LocalPath::new(self.as_str().split_at(final_end_position).1))
    }

    /*
    First remove ending "/"

    If the path ends with ".." or ".", then

    Return values:
    - (None, None)

    */

    pub fn parent(&self) -> Option<&LocalPath> {
        LocalPathReverseSegmentersIterator::new(self.as_str())
            .next()
            .and_then(|(seg, idx)| {
                if let LocalPathSegment::Root = seg {
                    return None;
                }

                Some(LocalPath::new(self.as_str().split_at(idx).0))
            })
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
        if let Some((LocalPathSegment::File(name), _)) =
            LocalPathReverseSegmentersIterator::new(self.as_str()).next()
        {
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

impl ToString for LocalPath {
    fn to_string(&self) -> String {
        self.as_str().to_owned()
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

impl AsRef<OsStr> for LocalPath {
    fn as_ref(&self) -> &OsStr {
        self.as_str().as_ref()
    }
}

impl AsRef<OsStr> for LocalPathBuf {
    fn as_ref(&self) -> &OsStr {
        self.as_str().as_ref()
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

/// Iterates over a path's segments in forward order.
/// On each iteration yields the next segment and the end position of that
struct LocalPathSegmentIterator<'a> {
    remaining: &'a str,
    absolute_position: usize,
}

impl<'a> LocalPathSegmentIterator<'a> {
    fn new(remaining: &'a str) -> Self {
        Self {
            remaining,
            absolute_position: 0,
        }
    }
}

impl<'a> Iterator for LocalPathSegmentIterator<'a> {
    /// Next segment and the end position of it in the original string.
    type Item = (LocalPathSegment<'a>, usize);

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining.is_empty() {
            return None;
        }

        let starting_len = self.remaining.len();

        if self.absolute_position == 0 {
            if let Some(r) = self.remaining.strip_prefix(SEGMENT_DELIMITER) {
                self.remaining = r.trim_start_matches(SEGMENT_DELIMITER);
                self.absolute_position += starting_len - self.remaining.len();
                return Some((LocalPathSegment::Root, self.absolute_position));
            }
        }

        let (cur, rest) = self
            .remaining
            .split_once(SEGMENT_DELIMITER)
            .unwrap_or_else(|| (self.remaining, ""));
        self.remaining = rest.trim_start_matches(SEGMENT_DELIMITER);
        self.absolute_position += starting_len - self.remaining.len();

        let segment = {
            if cur == "." {
                LocalPathSegment::CurrentDir
            } else if cur == ".." {
                LocalPathSegment::ParentDir
            } else {
                LocalPathSegment::File(cur)
            }
        };

        Some((segment, self.absolute_position))
    }
}

/// Iterates over a path's segments in reverse order.
/// On each iteration this yields the next segment and the start position of it
/// in the original string.
struct LocalPathReverseSegmentersIterator<'a> {
    remaining: &'a str,
}

impl<'a> LocalPathReverseSegmentersIterator<'a> {
    fn new(remaining: &'a str) -> Self {
        Self { remaining }
    }
}

impl<'a> Iterator for LocalPathReverseSegmentersIterator<'a> {
    /// Previous segment and the start position of that segment in the original
    /// string.
    type Item = (LocalPathSegment<'a>, usize);

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining.is_empty() {
            return None;
        }

        self.remaining = self.remaining.trim_end_matches(SEGMENT_DELIMITER);
        if self.remaining.is_empty() {
            return Some((LocalPathSegment::Root, 0));
        }

        // Split "parent/child" into ("parent/", "child")
        // Or "child" into ("", "child")
        let (begin, tail) = self.remaining.split_at(
            self.remaining
                .rfind(SEGMENT_DELIMITER)
                .map(|i| i + 1)
                .unwrap_or(0),
        );
        self.remaining = begin;

        let pos = self.remaining.len();
        if tail == "." {
            Some((LocalPathSegment::CurrentDir, pos))
        } else if tail == ".." {
            Some((LocalPathSegment::ParentDir, pos))
        } else {
            Some((LocalPathSegment::File(tail), pos))
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;

    #[test]
    fn path_segments_test() {
        let test_cases: [(&'static str, &'static [LocalPathSegment<'static>]); _] = [
            ("", &[]),
            ("/", &[LocalPathSegment::Root]),
            ("//", &[LocalPathSegment::Root]),
            ("///", &[LocalPathSegment::Root]),
            (
                "/..",
                &[LocalPathSegment::Root, LocalPathSegment::ParentDir],
            ),
            (
                "/../",
                &[LocalPathSegment::Root, LocalPathSegment::ParentDir],
            ),
            ("hello", &[LocalPathSegment::File("hello")]),
            (
                "/../hello",
                &[
                    LocalPathSegment::Root,
                    LocalPathSegment::ParentDir,
                    LocalPathSegment::File("hello"),
                ],
            ),
            (
                "/..hello",
                &[LocalPathSegment::Root, LocalPathSegment::File("..hello")],
            ),
            (
                "/.hello",
                &[LocalPathSegment::Root, LocalPathSegment::File(".hello")],
            ),
            (
                "/./hello",
                &[
                    LocalPathSegment::Root,
                    LocalPathSegment::CurrentDir,
                    LocalPathSegment::File("hello"),
                ],
            ),
            (".", &[LocalPathSegment::CurrentDir]),
            (
                ".//.",
                &[LocalPathSegment::CurrentDir, LocalPathSegment::CurrentDir],
            ),
            (
                "/hello//world////",
                &[
                    LocalPathSegment::Root,
                    LocalPathSegment::File("hello"),
                    LocalPathSegment::File("world"),
                ],
            ),
        ];

        for (path, expected_segments) in test_cases {
            let segments = LocalPath::new(path).segments().collect::<Vec<_>>();
            assert_eq!(
                &segments[..],
                expected_segments,
                "while testing \"{}\"",
                path
            );

            let mut rsegments = LocalPath::new(path).rsegments().collect::<Vec<_>>();
            rsegments.reverse();

            assert_eq!(
                &rsegments[..],
                expected_segments,
                "while testing \"{}\"",
                path
            );
        }
    }

    #[test]
    fn path_join_test() {
        assert_eq!(LocalPath::new("/").join("hello"), "/hello");
        assert_eq!(LocalPath::new("/var").join("/opt"), "/opt");
        assert_eq!(
            LocalPath::new("relative/path").join("to/something"),
            "relative/path/to/something"
        );

        assert_eq!(LocalPath::new("/var/").join("run"), "/var/run");

        assert_eq!(LocalPath::new("").join("file"), "file");
        assert_eq!(LocalPath::new("").join("/var"), "/var");

        assert_eq!(LocalPath::new("file").join("/var"), "/var");
    }

    #[test]
    fn path_strip_prefix_test() {
        let test_cases = [
            ("/a/b/c", "/a", Some("b/c")),
            ("/apples/oranges", "/apples", Some("oranges")),
            ("/apples/oranges", "/apples/", Some("oranges")),
            ("/apples/oranges", "/apples/oranges", Some("")),
            ("/apples/oranges", "/apples/oranges/", Some("")),
            ("/apples/oranges", "/app", None),
            ("/apples/oranges", "apples", None),
            ("/apples/oranges", "", Some("/apples/oranges")),
            ("/apples/oranges", "/", Some("apples/oranges")),
        ];

        for (original_path, prefix, expected_suffix) in test_cases {
            assert_eq!(
                LocalPath::new(original_path)
                    .strip_prefix(prefix)
                    .map(|p| p.as_str()),
                expected_suffix
            );
        }
    }

    #[test]
    fn path_normalization_test() {
        let test_cases = [
            ("/file/", "/file"),
            ("file/", "file"),
            ("./file/", "file"),
            (".", "."),
            ("/", "/"),
            ("/.", "/"),
            ("", ""),
            ("/../../../", "/"),
            ("/../hello/../../world", "/world"),
            (
                "/../../hello/world/./jello/apples/../file/",
                "/hello/world/jello/file",
            ),
        ];

        for (original_path, normalized_path) in test_cases {
            assert_eq!(
                LocalPath::new(original_path).normalized().as_str(),
                normalized_path,
                "while testing \"{}\"",
                original_path
            );
        }
    }

    #[test]
    fn path_parent_test() {
        let test_cases = [
            ("/", None),
            ("", None),
            (".", Some("")),
            ("./file", Some("./")),
            ("file", Some("")), // TODO: Consider changing this to ".".
            ("file/second", Some("file/")),
            ("file//second", Some("file//")),
            ("file/./second", Some("file/./")),
            ("/file/./second", Some("/file/./")),
        ];

        for (original_path, expected_parent) in test_cases {
            assert_eq!(
                LocalPath::new(original_path).parent().map(|p| p.as_str()),
                expected_parent,
                "while testing \"{}\"",
                original_path
            );
        }
    }

    #[test]
    fn path_functions() {
        let mut p = LocalPath::new("/var/run/something.txt").to_owned();
        assert_eq!(p.extension(), Some("txt"));

        p.set_extension("rs");
        assert_eq!(p.as_str(), "/var/run/something.rs");
        assert_eq!(p.file_name(), Some("something.rs"));

        assert!(p.pop());
        assert_eq!(p.as_str(), "/var/run/");
        assert_eq!(p.file_name(), Some("run"));
    }
}
