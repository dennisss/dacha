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
    pattern: RegExp,

    /// Note that since we normalize paths to not end in '/', we don't check
    /// this as part of the regex matching.
    only_select_directories: bool,

    pending_directories: Vec<LocalPathBuf>,

    matched_files: Vec<LocalPathBuf>,
}

impl GlobIterator {
    pub fn create(pattern: &LocalPath) -> Result<Self> {
        let only_select_directories = pattern.as_str().ends_with("/");

        let mut pattern = pattern.to_owned();
        if !pattern.is_absolute() {
            pattern = crate::current_dir()?.join(pattern);
        }

        pattern = pattern.normalized();

        let segments = pattern.segments().collect::<Vec<_>>();
        if segments.is_empty() {
            todo!()
            // return Ok(());
        }

        // Convert the glob into a regexp
        let pattern = {
            let mut nodes = vec![];
            nodes.push(RegExpNode::Start);

            let mut first_file = true;

            for segment in segments {
                match segment {
                    LocalPathSegment::Root => {
                        nodes.push(RegExpNode::Literal(Char::Value('/')));
                    }
                    LocalPathSegment::File(mut name) => {
                        if !first_file {
                            nodes.push(RegExpNode::Literal(Char::Value('/')));
                        }
                        first_file = false;

                        // TODO: Append a

                        while !name.is_empty() {
                            if let Some(rest) = name.strip_prefix("**") {
                                // '**' => '.*'
                                nodes.push(RegExpNode::Quantified {
                                    node: Box::new(RegExpNode::Literal(Char::Wildcard)),
                                    quantifier: Quantifier::ZeroOrMore,
                                    greedy: true,
                                });
                                name = rest;
                            } else if let Some(rest) = name.strip_prefix("*") {
                                // '*' == '[^/]*'
                                nodes.push(RegExpNode::Quantified {
                                    node: Box::new(RegExpNode::Class {
                                        chars: vec![Char::Value('/')],
                                        inverted: true,
                                    }),
                                    quantifier: Quantifier::ZeroOrMore,
                                    greedy: true,
                                });
                                name = rest;
                            } else {
                                let mut iter = name.char_indices();

                                let (_, c) = iter.next().unwrap();

                                if c == '?' {
                                    nodes.push(RegExpNode::Literal(Char::Wildcard));
                                } else {
                                    nodes.push(RegExpNode::Literal(Char::Value(c)));
                                }

                                name = name.strip_prefix(c).unwrap();
                            }
                        }
                    }
                    LocalPathSegment::CurrentDir | LocalPathSegment::ParentDir => {
                        panic!("Should not exist in normalized paths.")
                    }
                }
            }

            nodes.push(RegExpNode::End);

            RegExpNode::Expr(nodes.into_iter().map(|n| Box::new(n)).collect())
        };

        let regexp = RegExp::new_from_parsed(Box::new(pattern), Flags::empty())?;

        Ok(Self {
            pattern: regexp,
            only_select_directories,
            pending_directories: vec![LocalPath::new("/").to_owned()],
            matched_files: vec![],
        })
    }

    pub async fn next(&mut self) -> Result<Option<LocalPathBuf>> {
        while self.matched_files.is_empty() && !self.pending_directories.is_empty() {
            let dir = self.pending_directories.pop().unwrap();

            for entry in crate::read_dir(&dir)? {
                let path = dir.join(entry.name());
                let is_dir = entry.typ() == FileType::Directory;

                if self.pattern.test(path.as_str()) && (!self.only_select_directories || is_dir) {
                    self.matched_files.push(path.clone());
                }

                if is_dir {
                    let inner_path = format!("{}/", path.as_str());
                    if self.pattern.test_prefix(inner_path) {
                        self.pending_directories.push(path);
                    }
                }
            }
        }

        // TODO: Return in a sorted order.
        Ok(self.matched_files.pop())
    }

    /// NOTE: The given path must be absolute.
    pub fn matches_file(&self, path: &LocalPath) -> bool {
        self.pattern.test(path.as_str())
    }
}
