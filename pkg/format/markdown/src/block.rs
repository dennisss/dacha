use crate::block_builder::BlockBuilder;
use crate::encoding::encode_html_text;
use crate::inline::{InlineContent, InlineElement};

/// Single node in a Markdown source code parse tree.
#[derive(Clone, Debug)]
pub enum Block {
    /// Top level node of the tree.
    /// There will always be exactly one of these.
    Document {
        children: Vec<Box<Block>>,
    },

    List {
        first_prefix: ListItemPrefix,
        tight: bool,
        children: Vec<Box<Block>>,
    },

    /// NOTE: When parsed, ListItems will always appear directly under a List.
    ListItem {
        marker: ListMarker,
        children: Vec<Box<Block>>,

        /// If false, then some of the children in 'children' have blank lines
        /// separating them.
        tight: bool,
    },

    BlockQuote {
        children: Vec<Box<Block>>,
    },

    Paragraph {
        children: InlineContent,
    },

    /*

    hello
    # world # **hello**   ###########
    */
    Heading {
        marker: HeadingMarker,
        children: InlineContent,
    },

    ThematicBreak,

    // Keeps consuming lines until we don't see an empty one or more indented lines
    IndentedCodeBlock {
        code: String,
    },

    FencedCodeBlock {
        fence: String,
        info: String,

        /// Number of spaces at the beginning of the opening fence. These space
        /// are removed from all lines of the code.
        indent: usize,

        code: String,

        closed: bool,
    },
}

impl Block {
    pub fn parse_document(text: &str) -> Self {
        let mut doc = BlockBuilder::new_document();

        for line in text.lines() {
            assert!(doc.append_line(line)); // Should never fail.
        }

        doc.close()
    }

    pub(crate) fn children_mut(&mut self) -> Option<&mut Vec<Box<Block>>> {
        match self {
            Block::Document { children }
            | Block::List { children, .. }
            | Block::ListItem { children, .. }
            | Block::BlockQuote { children } => Some(children),
            _ => None,
        }
    }

    pub fn to_html(&self) -> String {
        self.to_html_impl(false)
    }

    fn to_html_impl(&self, tight: bool) -> String {
        match self {
            Block::Document { children } => {
                format!("{}", Self::to_html_slice(&children, false))
            }
            Block::List {
                first_prefix,
                tight,
                children,
            } => {
                let tag = first_prefix.number().map(|_| "ol").unwrap_or("ul");

                let start = first_prefix
                    .number()
                    .and_then(|num| {
                        if num == 1 {
                            None
                        } else {
                            Some(format!(" start=\"{}\"", num))
                        }
                    })
                    .unwrap_or(String::new());

                let mut out = String::new();

                format!(
                    "<{tag}{start}>\n{inner}</{tag}>",
                    tag = tag,
                    start = start,
                    inner = Self::to_html_slice(&children, *tight)
                )
            }
            Block::ListItem {
                marker, children, ..
            } => {
                format!("<li>{}</li>", Self::to_html_slice_inline(&children, tight))
            }
            Block::BlockQuote { children } => {
                format!(
                    "<blockquote>\n{}</blockquote>",
                    Self::to_html_slice(&children, false)
                )
            }
            Block::Paragraph { children } => {
                if tight {
                    children.to_html()
                } else {
                    format!("<p>{}</p>", children.to_html())
                }
            }
            Block::Heading { marker, children } => {
                format!(
                    "<h{num}>{inner}</h{num}>",
                    num = marker.level,
                    inner = children.to_html()
                )
            }
            Block::IndentedCodeBlock { code } => {
                format!("<pre><code>{}</code></pre>", encode_html_text(&code))
            }
            Block::FencedCodeBlock {
                fence,
                code,
                info,
                indent,
                closed,
            } => {
                let info = {
                    if info.is_empty() {
                        String::new()
                    } else {
                        format!(" class=\"language-{}\"", encode_html_text(info))
                    }
                };

                format!(
                    "<pre><code{info}>{inner}</code></pre>",
                    info = info,
                    inner = encode_html_text(&code)
                )
            }
            Block::ThematicBreak => "<hr />".into(),
        }
    }

    fn to_html_slice_inline(blocks: &[Box<Self>], tight: bool) -> String {
        // Do not append "\n" before inline text:
        // - e.g. prefer "<li>text</li>" over "<li>\ntext</li>" or "<li>"
        // - but we do want to output "<li>\n<p>text</p>\n</li>"
        if tight {
            let mut out = String::new();

            let mut final_inline = true;

            for i in 0..blocks.len() {
                if i == 0 && let Block::Paragraph { .. } = blocks[i].as_ref() {
                    //
                    final_inline = true;
                } else {
                    final_inline = false;
                    out.push('\n');
                }

                out.push_str(&blocks[i].to_html_impl(tight));
            }

            if !final_inline {
                out.push('\n');
            }

            return out;
        }

        format!("\n{}", Self::to_html_slice(blocks, tight))
    }

    fn to_html_slice(blocks: &[Box<Self>], tight: bool) -> String {
        let mut out = String::new();

        let mut i = 0;
        while i < blocks.len() {
            out.push_str(&blocks[i].to_html_impl(tight));
            out.push('\n');
            i += 1;
        }

        out
    }
}

#[derive(Clone, Debug)]
pub struct ListMarker {
    pub prefix: ListItemPrefix,

    /// Number of characters from the very left of the line to the first text
    /// character in the list item.
    ///
    /// Future lines must be indented by at least this much to be considered to
    /// be part of the same list item.
    pub size: usize,
}

#[derive(Clone, Debug)]
pub enum ListItemPrefix {
    Bullet(char),
    Ordered { number: usize, separator: char },
}

impl ListItemPrefix {
    pub fn compatible_with(&self, other: &ListItemPrefix) -> bool {
        self.normalized_prefix() == other.normalized_prefix()
    }

    fn normalized_prefix(&self) -> char {
        match self {
            &ListItemPrefix::Bullet(c) => c,
            &ListItemPrefix::Ordered { number, separator } => separator,
        }
    }

    pub fn number(&self) -> Option<usize> {
        match self {
            &ListItemPrefix::Bullet(_) => None,
            &ListItemPrefix::Ordered { number, separator } => Some(number),
        }
    }
}

#[derive(Clone, Debug)]
pub struct HeadingMarker {
    pub level: usize,
}
