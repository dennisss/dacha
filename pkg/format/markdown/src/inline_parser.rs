use std::collections::HashMap;

use crate::{InlineContent, InlineElement};

/// Characters which are allowed to be escaped witha backslash in markdown.
const ASCII_PUNCTUATION: &'static str = "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~";

/// Interprets an inline string as a sequence of InlineElements.
pub struct InlineContentParser {
    elements: Vec<ParsedInlineElement>,
    current_text_span: String,
}

struct ParsedInlineElement {
    element: InlineElement,

    /// May be set for Text elements which may later be interpreted as a
    /// delimiter of another type of element.
    delimiter: Option<Delimiter>,
}

struct Delimiter {
    typ: DelimiterType,
    active: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DelimiterType {
    Emphasis { can_open: bool, can_close: bool },
    StartLink,
    StartImage,
}

/// Wrapper around a char which may have been backslash escaped in the markdown
/// text.
#[derive(Clone, Copy)]
struct Character {
    value: char,
    escaped: bool,
}

impl Character {
    fn len(&self) -> usize {
        self.value.len_utf8() + if self.escaped { 1 } else { 0 }
    }
}

impl InlineContentParser {
    pub fn parse(text: &str) -> InlineContent {
        let mut inst = Self {
            elements: vec![],
            current_text_span: String::new(),
        };

        inst.parse_impl(text);

        Self::parse_emphasis(inst.elements)
    }

    fn parse_impl(&mut self, mut text: &str) {
        // The character before the current run of characters.
        // Updates in each iteration on the main while loop below.
        let mut prev_char = ' ';

        // The last character that was parsed.
        // Should be updated whenever we advance the text.
        let mut next_prev_char = ' ';

        while let Some((c, rest)) = Self::next_character(text) {
            prev_char = next_prev_char;

            next_prev_char = c.value;
            text = rest;

            if !c.escaped && (c.value == '*' || c.value == '_' || c.value == '`') {
                // Close previous span.
                self.close_text_span(None);

                self.current_text_span.push(c.value);

                while let Some((c_next, rest)) = Self::next_character(text) {
                    if !c_next.escaped && c_next.value == c.value {
                        text = rest;
                        next_prev_char = c.value;
                        self.current_text_span.push(c_next.value);
                    } else {
                        break;
                    }
                }

                // Code spans have higher precedence than other delimiters and ignore escaping
                // of characters so we need to parse it immediately.
                if c.value == '`' {
                    // Find the matching end delimiter run (must be a run of backticks that is the
                    // same length as the opening delimiter). Backslash escaping is not respected.
                    let mut closer = None;
                    {
                        regexp!(PATTERN => "`+");

                        let mut current_match = PATTERN.exec(text);
                        while let Some(m) = current_match {
                            if m.group(0).unwrap().len() != self.current_text_span.len() {
                                current_match = m.next();
                                continue;
                            }

                            closer =
                                Some((text.split_at(m.index()).0, text.split_at(m.last_index()).1));
                            break;
                        }
                    }

                    if let Some((mut code, rest)) = closer {
                        let mut code = code.replace('\n', " ");

                        // Strip at most 1 symmetric space from front end end of the code (if the
                        // entire thing isn't whitespace).
                        if let Some(c1) = code.strip_prefix(' ') {
                            if let Some(c2) = c1.strip_suffix(' ') {
                                if !c2.trim().is_empty() {
                                    code = c2.to_string();
                                }
                            }
                        }

                        text = rest;
                        next_prev_char = '`';
                        self.current_text_span.clear();
                        self.elements.push(ParsedInlineElement {
                            element: InlineElement::CodeSpan(code),
                            delimiter: None,
                        });
                    } else {
                        // Treat as a regular text segment.
                        self.close_text_span(None);
                    }

                    continue;
                }

                let next_char = match Self::next_character(text) {
                    Some((c, _)) => c.value,
                    None => ' ',
                };

                let left_flanking = !next_char.is_ascii_whitespace()
                    && (!ASCII_PUNCTUATION.contains(next_char)
                        || prev_char.is_ascii_whitespace()
                        || ASCII_PUNCTUATION.contains(prev_char));

                let right_flanking = !prev_char.is_ascii_whitespace()
                    && (!ASCII_PUNCTUATION.contains(prev_char)
                        || next_char.is_ascii_whitespace()
                        || ASCII_PUNCTUATION.contains(next_char));

                let can_open = {
                    if self.current_text_span.starts_with("*") {
                        left_flanking
                    } else {
                        // starts with '_'
                        left_flanking && (!right_flanking || ASCII_PUNCTUATION.contains(prev_char))
                    }
                };

                let can_close = {
                    if self.current_text_span.starts_with("*") {
                        right_flanking
                    } else {
                        // starts with '_'
                        right_flanking && (!left_flanking || ASCII_PUNCTUATION.contains(next_char))
                    }
                };

                self.close_text_span(Some(Delimiter {
                    typ: DelimiterType::Emphasis {
                        can_open,
                        can_close,
                    },
                    active: true,
                }));
                continue;
            }

            if !c.escaped && c.value == '!' {
                if let Some((
                    Character {
                        value: '[',
                        escaped: false,
                    },
                    rest,
                )) = Self::next_character(text)
                {
                    // Close previous span.
                    self.close_text_span(None);

                    self.current_text_span.push_str("![");
                    text = rest;
                    next_prev_char = '[';

                    self.close_text_span(Some(Delimiter {
                        typ: DelimiterType::StartImage,
                        active: true,
                    }));

                    continue;
                }
            }

            if !c.escaped && c.value == '[' {
                // Close previous span.
                self.close_text_span(None);

                self.current_text_span.push(c.value);

                self.close_text_span(Some(Delimiter {
                    typ: DelimiterType::StartLink,
                    active: true,
                }));

                continue;
            }

            if let Character {
                value: ']',
                escaped: false,
            } = c
            {
                if let Some(rest) = self.parse_link(text) {
                    text = rest;
                    next_prev_char = ')'; // Final character in a link.
                    continue;
                }
            }

            if c.value == '\n' {
                self.close_text_span(None);
                self.elements.push(ParsedInlineElement {
                    element: InlineElement::SoftBreak,
                    delimiter: None,
                });
                continue;
            }

            // Hard Break: Two or more spaces followed by '\n' (or soft break if we have
            // less than two) This also generally handles all whitespace spans
            // to avoid retrying this logic later.
            if c.value == ' ' {
                // TODO: Instead just push to self.current_text_span and truncate if needed.
                let mut deferred_text = String::new();

                deferred_text.push(c.value);

                let mut two_or_more_spaces = false;
                while let Some((Character { value: ' ', .. }, rest)) = Self::next_character(text) {
                    text = rest;
                    next_prev_char = ' ';

                    deferred_text.push(' ');
                    two_or_more_spaces = true;
                }

                let mut got_newline = false;
                if let Some((Character { value: '\n', .. }, rest)) = Self::next_character(text) {
                    text = rest;
                    next_prev_char = ' ';
                    got_newline = true;
                }

                if got_newline {
                    self.close_text_span(None);

                    self.elements.push(ParsedInlineElement {
                        element: if two_or_more_spaces {
                            InlineElement::HardBreak
                        } else {
                            InlineElement::SoftBreak
                        },
                        delimiter: None,
                    });
                } else {
                    self.current_text_span.push_str(&deferred_text);
                }

                continue;
            }

            // Hard Break: '\' followed by '\n'
            if c.value == '\\' && let Some(rest) = text.strip_prefix('\n') {
                text = rest;
                next_prev_char = '\n';

                self.close_text_span(None);
                self.elements.push(ParsedInlineElement {
                    element: InlineElement::HardBreak,
                    delimiter: None,
                });

                continue;
            }

            // Otherwise, just a normal character
            self.current_text_span.push(c.value);
        }

        self.close_text_span(None);
    }

    /// Called when we have found a ']' delimiter to fully parse a link or
    /// image.
    ///
    /// This basically uses the 'look for link or image' algorithm described in
    /// the Commonmark spec.
    ///
    /// Arguments:
    /// - rest: The remaining text after the ']'
    ///
    /// On success, we will return Some(rest) which is the remaining text after
    /// the link/image and we will push an appropriate element to self.elements,
    /// otherwise, we will None.
    fn parse_link<'a>(&mut self, mut rest: &'a str) -> Option<&'a str> {
        // Find matching '[' or '!['
        let (start_index, is_image) = {
            let mut idx = None;
            for i in (0..self.elements.len()).rev() {
                if let Some(d) = &self.elements[i].delimiter {
                    if d.typ != DelimiterType::StartImage && d.typ != DelimiterType::StartLink {
                        continue;
                    }

                    if !d.active {
                        self.elements[i].delimiter = None;
                        return None;
                    }

                    idx = Some((i, d.typ == DelimiterType::StartImage));
                    break;
                }
            }

            match idx {
                Some(v) => v,
                None => return None,
            }
        };

        // Look ahead to find the link

        // Take one unescaped '(' character.
        match Self::next_character(rest) {
            Some((
                Character {
                    value: '(',
                    escaped: false,
                },
                r,
            )) => {
                rest = r;
            }
            _ => return None,
        }

        rest = rest.trim_start();

        let mut link = String::new();
        if let Some((
            Character {
                value: '<',
                escaped: false,
            },
            r,
        )) = Self::next_character(rest)
        {
            rest = r;

            while let Some((c, r)) = Self::next_character(rest) {
                rest = r;

                if !c.escaped && c.value == '>' {
                    break;
                }

                link.push(c.value);
            }
        } else {
            let mut paren_level = 0;
            while let Some((c, r)) = Self::next_character(rest) {
                if let Character {
                    value: ')',
                    escaped: false,
                } = c
                {
                    if paren_level == 0 {
                        break;
                    } else {
                        paren_level -= 1;
                    }
                }

                if let Character {
                    value: '(',
                    escaped: false,
                } = c
                {
                    paren_level += 1;
                }

                if c.value.is_ascii_whitespace() {
                    break;
                }

                if c.value.is_ascii_control() {
                    return None;
                }

                link.push(c.value);
                rest = r;
            }
        }

        rest = rest.trim_start();

        let mut title = None;
        if let Some((c, r)) = Self::next_character(rest) && c.value != ')' {
            rest = r;

            if c.escaped {
                return None;
            }

            let ender = match c.value {
                '"' => '"',
                '\'' => '\'',
                '(' => ')',
                _ => return None,
            };

            let mut text = String::new();
            loop {
                match Self::next_character(rest) {
                    Some((c, r)) => {
                        rest = r;

                        if !c.escaped && c.value == ender {
                            break;
                        }

                        text.push(c.value);
                    }
                    None => return None,
                }
            }

            title = Some(text);
        }

        // TODO:

        rest = rest.trim_start();

        // Take one unescaped ')' character.
        match Self::next_character(rest) {
            Some((
                Character {
                    value: ')',
                    escaped: false,
                },
                r,
            )) => {
                rest = r;
            }
            _ => return None,
        }

        self.close_text_span(None);

        // Take all the elements and process emphasis on the rest.
        let text = Self::parse_emphasis(self.elements.split_off(start_index + 1));

        // Pop the '[' / '!['
        self.elements.pop();

        self.elements.push(ParsedInlineElement {
            element: InlineElement::Link {
                is_image,
                text,
                link,
                title,
            },
            delimiter: None,
        });

        // Prevent links inside of links.
        for element in &mut self.elements {
            if let Some(d) = &mut element.delimiter {
                if d.typ == DelimiterType::StartLink {
                    d.active = false;
                }
            }
        }

        Some(rest)
    }

    /// Transforms the elements by replacing all valid emphasis delimiter spans
    /// Emphasis inline elements.
    ///
    /// This is essentially the 'process emphasis' algorithm in the Commonmark
    /// spec.
    fn parse_emphasis(mut raw_elements: Vec<ParsedInlineElement>) -> InlineContent {
        // Furthest back index in the elements we will look through in order to find
        // matching opening delimiters to some closing delimiter of the same type.
        let mut openers_bottom = HashMap::new();
        // openers_bottom.insert('*', 0);
        // openers_bottom.insert('_', 0);

        let mut current_pos = 0;
        while current_pos < raw_elements.len() {
            // Skip forward until we are positioned over a 'closer' emphasis delimiter.
            let (marker, closer_len) =
                match Self::find_emphasis_delim(&raw_elements[current_pos], false, true) {
                    Some(v) => v,
                    None => {
                        // TODO: If an entry in openers_bottom is equal to current_pos, increment it
                        // by one as well.

                        current_pos += 1;
                        continue;
                    }
                };

            // Look backwards to find the closest 'opener'
            let mut opener_i = None;
            let bottom_idx = openers_bottom
                .get(&(marker, closer_len % 3))
                .cloned()
                .unwrap_or(0);
            for i in (bottom_idx..current_pos).rev() {
                let (marker_i, len_i) =
                    match Self::find_emphasis_delim(&raw_elements[i], true, false) {
                        Some(v) => v,
                        None => continue,
                    };

                if marker_i == marker {
                    opener_i = Some(i);
                    break;
                }
            }

            let opener_i = match opener_i {
                Some(v) => v,
                None => {
                    // Didn't find an opener so, next time don't look backwards so far.
                    openers_bottom.insert((marker, closer_len % 3), current_pos);
                    current_pos += 1;
                    continue;
                }
            };

            let opener_text = Self::get_text_mut(&mut raw_elements[opener_i]);
            let opener_len = opener_text.len();

            let closer_text = Self::get_text_mut(&mut raw_elements[current_pos]);
            let closer_len = closer_text.len();

            let is_strong = opener_len >= 2 && closer_len >= 2;

            // Handle closing delimiter removal.
            {
                closer_text.pop();
                if is_strong {
                    closer_text.pop();
                }

                if closer_text.is_empty() {
                    raw_elements.remove(current_pos);
                }
            }

            // Wrap inner elements in emphasis.
            {
                let mut inner = vec![];
                for i in (opener_i + 1)..current_pos {
                    inner.push(raw_elements.remove(opener_i + 1).element);
                }

                current_pos -= inner.len() - 1;

                let new_el = ParsedInlineElement {
                    element: InlineElement::Emphasis {
                        text: InlineContent { elements: inner },
                        strong: is_strong,
                    },
                    delimiter: None,
                };

                raw_elements.insert(opener_i + 1, new_el);
            }

            // Handle opener delimiter removal
            {
                let opener_text = Self::get_text_mut(&mut raw_elements[opener_i]);

                opener_text.pop();
                if is_strong {
                    opener_text.pop();
                }

                if opener_text.is_empty() {
                    raw_elements.remove(opener_i);
                    current_pos -= 1;
                }
            }
        }

        InlineContent {
            elements: raw_elements.into_iter().map(|e| e.element).collect(),
        }
    }

    fn find_emphasis_delim(
        el: &ParsedInlineElement,
        opener: bool,
        closer: bool,
    ) -> Option<(char, usize)> {
        let delim = match &el.delimiter {
            Some(d) => d,
            None => return None,
        };

        match &delim.typ {
            DelimiterType::Emphasis {
                can_open,
                can_close,
            } => {
                if (opener && !*can_open) || (closer && !*can_close) {
                    return None;
                }
            }
            _ => return None,
        };

        let text = match &el.element {
            InlineElement::Text(t) => t.as_str(),
            _ => panic!(), // Delimiters always use text elements.
        };

        let marker = text.chars().next().unwrap();

        Some((marker, text.len()))
    }

    fn get_text_mut(el: &mut ParsedInlineElement) -> &mut String {
        match &mut el.element {
            InlineElement::Text(t) => t,
            _ => panic!(), // Delimiters always use text elements.
        }
    }

    fn close_text_span(&mut self, delimiter: Option<Delimiter>) {
        if self.current_text_span.is_empty() {
            return;
        }

        self.elements.push(ParsedInlineElement {
            element: InlineElement::Text(self.current_text_span.split_off(0)),
            delimiter,
        });
    }

    /// Gets the next full character from the text and returns the remaining
    /// text after the character.
    fn next_character(text: &str) -> Option<(Character, &str)> {
        Self::next_character_inner(text).map(|c| (c, text.split_at(c.len()).1))
    }

    fn next_character_inner(text: &str) -> Option<Character> {
        let mut chars = text.chars();

        let c = match chars.next() {
            Some(c) => c,
            None => return None,
        };

        if c == '\\' {
            if let Some(c2) = chars.next() {
                if ASCII_PUNCTUATION.contains(c2) {
                    return Some(Character {
                        value: c2,
                        escaped: true,
                    });
                }
            }
        }

        Some(Character {
            value: c,
            escaped: false,
        })
    }
}
