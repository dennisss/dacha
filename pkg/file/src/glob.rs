use alloc::boxed::Box;
use alloc::vec::Vec;
use std::borrow::ToOwned;
use sys::FileType;

use automata::regexp::node::Quantifier;
use automata::regexp::node::{Char, RegExpNode};
use automata::regexp::vm::flags::Flags;
use automata::regexp::vm::instance::RegExp;
use common::errors::*;

use crate::{LocalPath, LocalPathBuf, LocalPathSegment};

pub struct FileFilterDecision {
    /// If true, this file should be emitted by the iterator.
    pub emit: bool,

    /// If true and the file is a directory, continue traversing files in this directory.
    pub traverse: bool,
}

pub trait FileFilter: Send + Sync {
    fn filter_file(&self, path: &LocalPath, is_dir: bool) -> FileFilterDecision;
}

/// An iterator that recursively traverses all files in a directory.
///
/// TODO: Eventually merge with the GlobIterator after we have more unit tests.
pub struct FileIterator {
    pending_directories: Vec<LocalPathBuf>,
    matched_files: Vec<LocalPathBuf>,
    filter: Box<dyn FileFilter>
}

impl FileIterator {
    pub fn new(base_dir: &LocalPath, filter: Box<dyn FileFilter>) -> Self {
        Self {
            filter,
            pending_directories: vec![base_dir.to_owned()],
            matched_files: vec![],
        }
    }

    pub async fn next(&mut self) -> Result<Option<LocalPathBuf>> {
        while self.matched_files.is_empty() && !self.pending_directories.is_empty() {
            let dir = self.pending_directories.pop().unwrap();

            for entry in crate::read_dir(&dir)? {
                let path = dir.join(entry.name());
                let is_dir = entry.typ() == FileType::Directory;

                let decision = self.filter.filter_file(&path, is_dir);

                if decision.emit {
                    self.matched_files.push(path.clone());
                }

                if is_dir && decision.traverse {
                    self.pending_directories.push(path);
                }
            }
        }

        // TODO: Return in a sorted order.
        Ok(self.matched_files.pop())
    }

    /// NOTE: The given path must be absolute.
    pub fn matches_file(&self, path: &LocalPath) -> bool {
        self.filter.filter_file(path, false).emit
    }
}


pub struct GlobFileFilter {
    pattern: RegExp,

    /// Note that since we normalize paths to not end in '/', we don't check
    /// this as part of the regex matching.
    only_select_directories: bool,
}

impl GlobFileFilter {
    pub fn create(pattern: &LocalPath) -> Result<Self> {
        let only_select_directories = pattern.to_str().unwrap().ends_with("/");

        let mut pattern = pattern.to_owned();
        if !pattern.is_absolute() {
            pattern = crate::current_dir()?.join(pattern);
        }

        pattern = pattern.normalize_lexically()?;

        let regexp = Self::compile_glob(&pattern)?;

        Ok(Self {
            pattern: regexp,
            only_select_directories,
        })
    }

    /// NOTE: This assumes that the path is normalized and absolute. This
    /// pattern does not handle matching of directories vs files.
    pub fn compile_glob(pattern: &LocalPath) -> Result<RegExp> {
        // Convert the glob into a regexp
        let pattern = {
            let mut nodes = vec![];
            nodes.push(RegExpNode::Start);

            let mut remaining = pattern.to_str().unwrap();
            while !remaining.is_empty() {
                if let Some(rest) = remaining.strip_prefix("**/") {
                    // '**/' => '(.*/)?'

                    nodes.push(RegExpNode::Quantified {
                        node: Box::new(RegExpNode::Expr(vec![
                            Box::new(RegExpNode::Quantified {
                                node: Box::new(RegExpNode::Literal(Char::Wildcard)),
                                quantifier: Quantifier::ZeroOrMore,
                                greedy: true,
                            }),
                            Box::new(RegExpNode::Literal(Char::Value('/'))),
                        ])),
                        quantifier: Quantifier::ZeroOrOne,
                        greedy: true
                    });

                    remaining = rest;

                } else if let Some(rest) = remaining.strip_prefix("**") {
                    // '**' => '.*'
                    nodes.push(RegExpNode::Quantified {
                        node: Box::new(RegExpNode::Literal(Char::Wildcard)),
                        quantifier: Quantifier::ZeroOrMore,
                        greedy: true,
                    });
                    remaining = rest;
                } else if let Some(rest) = remaining.strip_prefix("*") {
                    // '*' == '[^/]*'
                    nodes.push(RegExpNode::Quantified {
                        node: Box::new(RegExpNode::Class {
                            chars: vec![Char::Value('/')],
                            inverted: true,
                        }),
                        quantifier: Quantifier::ZeroOrMore,
                        greedy: true,
                    });
                    remaining = rest;
                } else {
                    let mut iter = remaining.char_indices();

                    let (_, c) = iter.next().unwrap();

                    if c == '?' {
                        nodes.push(RegExpNode::Literal(Char::Wildcard));
                    } else {
                        nodes.push(RegExpNode::Literal(Char::Value(c)));
                    }

                    remaining = remaining.strip_prefix(c).unwrap();
                }
            }

            nodes.push(RegExpNode::End);

            RegExpNode::Expr(nodes.into_iter().map(|n| Box::new(n)).collect())
        };

        let regexp = RegExp::new_from_parsed(Box::new(pattern), Flags::empty())?;
        Ok(regexp)
    }
}

impl FileFilter for GlobFileFilter {
    fn filter_file(&self, path: &LocalPath, is_dir: bool) -> FileFilterDecision {
        let path = path.to_str().unwrap();

        let emit = self.pattern.test(path) && (!self.only_select_directories || is_dir);

        let traverse = {
            if is_dir {
                let inner_path = format!("{}/", path);
                self.pattern.test_prefix(inner_path) 
            } else {
                false
            }
        };

        FileFilterDecision { emit, traverse }
    }
}

/// Finds files that match the given glob pattern.
///
/// Supported patterns:
/// - '*' matches any sequence of non-path delimiter ('/') characters in the
///   file path.
/// - '**' matches any sequence of characters in the path.
/// - 'dir/' only matches a directory named 'dir' (note that '/' must be the
///   last character in the pattern).
///
/// Internally this is implemented by converting the glob to a regular
/// expression that matches absolute paths. We continue recursively listing
/// directories while the regular expression partially matches on the directory
/// prefix.
pub struct GlobIterator {
    hidden: (),
}

impl GlobIterator {
    pub fn create(pattern: &LocalPath) -> Result<FileIterator> {
        let filter = Box::new(GlobFileFilter::create(pattern)?);

        // TODO: Can skip based on a shared prefix.
        let base_dir = LocalPath::new("/");

        Ok(FileIterator::new(base_dir, filter))
    }
}
