use crate::block::*;
use crate::inline::*;
use crate::inline_parser::InlineContentParser;

/// A block which is currently still being parsed as we haven't seen the end
/// yet. The block is incrementally built by adding more lines to it.
pub(crate) struct BlockBuilder {
    block: Block,

    /// For paragraphs, this is the accumulated text over multiple lines.
    inline_text: String,

    /// For lists, this tracks whether we just consumed an empty (whitespace
    /// only) line.
    last_empty_line: bool,

    /// For blocks which can contain other blocks, this is the last incomplete
    /// child parsed so far.
    child: Option<Box<BlockBuilder>>,
}

impl BlockBuilder {
    pub fn new_document() -> Self {
        Self {
            block: Block::Document { children: vec![] },
            inline_text: String::new(),
            last_empty_line: false,
            child: None,
        }
    }

    /// Starts a new open block given the first line in the block.
    ///
    /// This will consume the entire line in all cases.
    fn create(line: &str) -> Option<Box<Self>> {
        if line.trim().is_empty() {
            return None;
        }

        let (block, rest) = match Self::strip_indicator(line) {
            Some((v, rest)) => {
                // Required to avoid infinite recursion.
                assert!(rest.len() < line.len());

                (v, rest)
            }
            None => (
                Block::Paragraph {
                    children: InlineContent::default(),
                },
                line.trim_start(),
            ),
        };

        let mut inst = BlockBuilder {
            block,
            inline_text: String::new(),
            last_empty_line: false,
            child: None,
        };
        inst.append_line_remainder(rest);

        // Wrap list items in lists.
        if let Block::ListItem { marker, .. } = &inst.block {
            inst = BlockBuilder {
                block: Block::List {
                    first_prefix: marker.prefix.clone(),
                    tight: true,
                    children: vec![],
                },
                inline_text: String::new(),
                last_empty_line: false,
                child: Some(Box::new(inst)),
            }
        }

        Some(Box::new(inst))
    }

    /// Decodes the indicator at the start of a line which indicates a specific
    /// type of block.
    ///
    /// e.g. '1. ' at the start of a line indicates a list item.
    ///
    /// This will not return Paragraph blocks given they have no indicator.
    ///
    /// Returns an empty block of the type indicated and the remainder of the
    /// line after the indicator.
    fn strip_indicator<'a>(line: &'a str) -> Option<(Block, &'a str)> {
        // Technically this is not needed as this line has no indicators so would hit
        // the end of the function anyway.
        if line.is_empty() {
            return None;
        }

        if let Some(code) = Self::strip_indented_code_indicator(line) {
            return Some((
                Block::IndentedCodeBlock {
                    code: String::new(),
                },
                code,
            ));
        }

        if let Some(line) = Self::strip_block_quote_indicator(line) {
            return Some((Block::BlockQuote { children: vec![] }, line));
        }

        if Self::strip_thematic_break_indicator(line) {
            return Some((Block::ThematicBreak, ""));
        }

        if let Some((marker, line)) = Self::strip_list_item_indicator(line) {
            return Some((
                Block::ListItem {
                    marker,
                    children: vec![],
                    tight: true,
                },
                line,
            ));
        }

        if let Some((marker, line)) = Self::strip_atx_heading_indicator(line) {
            return Some((
                Block::Heading {
                    marker,
                    children: InlineContent::default(),
                },
                line,
            ));
        }

        if let Some(block) = Self::strip_code_fence_opener(line) {
            return Some((block, ""));
        }

        None
    }

    fn strip_code_fence_opener(mut line: &str) -> Option<Block> {
        // NOTE: Only up to 3 initial spaces are allowed so the line can't start with
        // tabs.
        regexp!(PATTERN => "^( {0,3})(`{3,}|~{3,})\\s*(.*?)\\s*$");

        let m = match PATTERN.exec(line) {
            Some(m) => m,
            None => return None,
        };

        let indent = m.group_str(1).unwrap().unwrap().len();
        let fence = m.group_str(2).unwrap().unwrap().to_string();
        let info = m.group_str(3).unwrap().unwrap().to_string();

        Some(Block::FencedCodeBlock {
            fence,
            info,
            indent,
            code: String::new(),
            closed: false,
        })
    }

    fn strip_thematic_break_indicator(mut line: &str) -> bool {
        line = match Self::strip_up_to_three_spaces(line) {
            Some(v) => v,
            None => return false,
        };

        regexp!(PATTERN => "^(?:(?:\\*\\s*){3,}|(?:-\\s*){3,}|(?:_\\s*){3,})\\s*$");
        PATTERN.exec(line).is_some()
    }

    fn strip_atx_heading_indicator(mut line: &str) -> Option<(HeadingMarker, &str)> {
        line = match Self::strip_up_to_three_spaces(line) {
            Some(v) => v,
            None => return None,
        };

        line = line.trim_end();

        regexp!(PATTERN => "^(#{1,6})\\s+(.*?)(?:\\s+#+)?$");

        let m = match PATTERN.exec(line) {
            Some(v) => v,
            None => return None,
        };

        let level = m.group(1).unwrap().len();
        let rest = m.group_str(2).unwrap().unwrap();

        Some((HeadingMarker { level }, rest))
    }

    fn strip_list_item_indicator(mut line: &str) -> Option<(ListMarker, &str)> {
        let original_len = line.len();

        line = match Self::strip_up_to_three_spaces(line) {
            Some(v) => v,
            None => return None,
        };

        regexp!(PATTERN => "^(?:([-+*])|(?:([0-9]{1,9}))([.\\)]))(\\s+|$)");

        let m = match PATTERN.exec(line) {
            Some(v) => v,
            None => return None,
        };

        let marker = m.group_str(0).unwrap().unwrap();
        let rest = line.split_at(marker.len()).1;

        let prefix = {
            if let Some(bullet) = m.group_str(1) {
                ListItemPrefix::Bullet(bullet.unwrap().chars().next().unwrap())
            } else {
                let num = m.group_str(2).unwrap().unwrap();
                let separator = m.group_str(3).unwrap().unwrap();

                ListItemPrefix::Ordered {
                    number: num.parse().unwrap(),
                    separator: separator.chars().next().unwrap(),
                }
            }
        };

        let mut size = original_len - rest.len();

        let spacing = m.group(4).unwrap();
        if rest.is_empty() {
            // If the first line of the list item ends in just whitespace, only one of the
            // spaces should count against the size.
            size -= spacing.len();
            size += 1;
        }

        // if rest.trim().is_empty()

        Some((ListMarker { prefix, size }, rest))
    }

    fn strip_indented_code_indicator(line: &str) -> Option<&str> {
        // TODO: Count a tab as 4 spaces (here and everywhere else in the indicator code
        // as well).
        line.strip_prefix("    ")
    }

    fn strip_block_quote_indicator(mut line: &str) -> Option<&str> {
        line = match Self::strip_up_to_three_spaces(line) {
            Some(v) => v,
            None => return None,
        };

        if let Some(mut line) = line.strip_prefix(">") {
            // Optionally strip a space
            line = line.strip_prefix(" ").unwrap_or(line);

            return Some(line);
        }

        None
    }

    /// Removes up to 3 spaces from the start of the line.
    fn strip_up_to_three_spaces(line: &str) -> Option<&str> {
        if line.starts_with("    ") {
            return None; // Has 4 spaces
        }

        Some(line.trim_start())
    }

    fn strip_n_spaces(mut line: &str, n: usize) -> Option<&str> {
        for _ in 0..n {
            if let Some(l) = line.strip_prefix(' ') {
                line = l;
            } else {
                return None;
            }
        }

        Some(line)
    }

    /// Attempts to append another new line of input to the block.
    ///
    /// Internally the implentation of this function decides whether or not the
    /// line is still relevant to this block type and if so, defers to
    /// append_line_remainder for actually adding the line (post indicator) to
    /// the block
    ///
    /// Returns whether or not the line was consumed (false means that the block
    /// is not closed).
    pub fn append_line(&mut self, line: &str) -> bool {
        match &mut self.block {
            Block::Document { .. } => {
                // Every line is part of the document.
                self.append_line_remainder(line);
                return true;
            }

            Block::List {
                first_prefix,
                tight,
                children,
            } => {
                self.append_line_remainder(line);
                self.last_empty_line = line.trim().is_empty();
                return self.child.is_some() || self.last_empty_line;
            }
            Block::ListItem { marker, .. } => {
                if let Some(rest) =
                    Self::allow_empty_line(|l| Self::strip_n_spaces(l, marker.size), line)
                {
                    self.append_line_remainder(rest);
                    self.last_empty_line = line.trim().is_empty();
                    return true;
                }
            }
            Block::BlockQuote { children } => {
                if let Some(rest) = Self::allow_empty_line(Self::strip_block_quote_indicator, line)
                {
                    self.append_line_remainder(rest);
                    return true;
                }
            }

            Block::Paragraph { children, .. } => {
                // Check for a setext heading.
                {
                    // If we find one, wrap the paragraph in a header.
                    regexp!(PATTERN => "^ {0,3}(?:\\=+|-+)\\s*$");
                    if PATTERN.exec(line).is_some() {
                        let level = if line.starts_with('=') { 1 } else { 2 };

                        self.block = Block::Heading {
                            marker: HeadingMarker { level },
                            children: children.clone(),
                        };

                        return true;
                    }
                }

                // Check if there is indication that another type of block should be starting.
                if let Some((block, _)) = Self::strip_indicator(line) {
                    match block {
                        Block::IndentedCodeBlock { .. } => {
                            // Can't interrupt a paragraph
                        }
                        Block::ListItem { marker, .. } => {
                            if marker.prefix.number().is_none() || marker.prefix.number() == Some(1)
                            {
                                return false;
                            }
                        }
                        _ => return false,
                    }
                }

                // - Whitespace only lines are treated as blank
                // - Can't interrupt a paragraph with an indented code block.
                let line = line.trim_start();
                if line.is_empty() {
                    return false;
                }

                self.inline_text.push('\n');
                self.append_line_remainder(line);
                return true;
            }
            Block::Heading { .. } | Block::ThematicBreak => {
                // Headings should only ever consume a single line
            }
            Block::IndentedCodeBlock { code } => {
                // Allow only lines that start with 4 spaces or consist only of whitespace.
                let rest = {
                    if let Some(rest) = Self::strip_indented_code_indicator(line) {
                        rest
                    } else if line.trim().is_empty() {
                        ""
                    } else {
                        return false;
                    }
                };

                self.append_line_remainder(rest);
                return true;
            }
            Block::FencedCodeBlock { closed, code, .. } => {
                if *closed {
                    return false;
                }

                self.append_line_remainder(line);

                // Include the '\n' in all but the closer line (it also doesn't get included for
                // the opener line as we don't call append_lin for the first line).
                if let Block::FencedCodeBlock { closed, code, .. } = &mut self.block {
                    if !*closed {
                        code.push('\n');
                    }
                }

                return true;
            }
        }

        false
    }

    fn allow_empty_line<F: Fn(&str) -> Option<&str>>(f: F, line: &str) -> Option<&str> {
        if line.is_empty() {
            return Some(line);
        }

        f(line)
    }

    /// Given that we know that a line belongs to the current block, appends the
    /// rest of the line ot the block (or its children).
    fn append_line_remainder(&mut self, rest: &str) {
        if let Some(mut child) = self.child.take() {
            if child.append_line(rest) {
                self.child = Some(child);
                return;
            }

            // Push the complete child.
            self.block
                .children_mut()
                .unwrap()
                .push(Box::new(child.close()));
        }

        match &mut self.block {
            Block::Document { .. } | Block::BlockQuote { .. } => {
                self.child = Self::create(rest);
            }
            Block::ListItem {
                children, tight, ..
            } => {
                self.child = Self::create(rest);

                if self.last_empty_line && self.child.is_some() && !children.is_empty() {
                    *tight = false;
                }
            }

            Block::List {
                first_prefix,
                tight,
                children,
            } => {
                if let Some((marker, rest)) = Self::strip_list_item_indicator(rest) {
                    if marker.prefix.compatible_with(first_prefix) {
                        if self.last_empty_line {
                            *tight = false;
                        }

                        let mut inst = BlockBuilder {
                            block: Block::ListItem {
                                marker,
                                children: vec![],
                                tight: true,
                            },
                            inline_text: String::new(),
                            last_empty_line: false,
                            child: None,
                        };
                        inst.append_line_remainder(rest);

                        self.child = Some(Box::new(inst));
                    }
                }
            }
            Block::Paragraph { children } | Block::Heading { children, .. } => {
                self.inline_text.push_str(rest);
            }
            Block::IndentedCodeBlock { code } => {
                let is_empty_line = rest.trim().is_empty();

                // Skip leading empty lines.
                if code.is_empty() && is_empty_line {
                    return;
                }

                self.inline_text.push_str(rest);
                self.inline_text.push('\n');

                // This will defer blank lines until we see the first non-blank line (so that we
                // skip trailing blank lines)
                if !is_empty_line {
                    code.push_str(&self.inline_text);
                    self.inline_text.clear();
                }
            }
            Block::FencedCodeBlock {
                fence,
                code,
                info,
                indent,
                closed,
            } => {
                // NOTE: If no closer is found, we will feed consuming lines until the end of
                // the file.
                {
                    if let Some(closer) = Self::strip_up_to_three_spaces(rest) {
                        if closer.trim_end() == fence.as_str() {
                            *closed = true;
                            return;
                        }
                    }
                }

                let mut rest = rest;
                for _ in 0..*indent {
                    rest = rest.strip_prefix(' ').unwrap_or(rest);
                }

                code.push_str(rest);
            }
            Block::ThematicBreak => {
                // The indicator consumes the entire line so no other content in
                // the line is expected.
            }
        }
    }

    /// Explictly mark the block as complete and retrieve all the parsed nodes.
    pub fn close(mut self) -> Block {
        if let Block::Paragraph { children } | Block::Heading { children, .. } = &mut self.block {
            *children = InlineContentParser::parse(&self.inline_text.trim_end());
        }

        if let Some(mut child) = self.child.take() {
            self.block
                .children_mut()
                .unwrap()
                .push(Box::new(child.close()));
        }

        // Any non-tight items in a list will make the whole list non-tight.
        if let Block::List {
            tight, children, ..
        } = &mut self.block
        {
            for c in children {
                if let Block::ListItem {
                    tight: item_tight, ..
                } = c.as_ref()
                {
                    if !*item_tight {
                        *tight = false;
                    }
                }
            }
        }

        self.block
    }
}
