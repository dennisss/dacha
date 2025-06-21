

#[derive(Default)]
pub struct TerminalTableBuilder {
    rows: Vec<Vec<String>>,
}

pub struct TerminalTableRowBuilder<'a> {
    row: &'a mut Vec<String>
}

impl TerminalTableBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn row<'a>(&'a mut self) -> TerminalTableRowBuilder<'a> {
        self.rows.push(vec![]);
        TerminalTableRowBuilder {
            row: self.rows.last_mut().unwrap()
        }
    }

    pub fn print(&self) {
        let mut col_sizes = vec![];
        for row in &self.rows {
            if col_sizes.len() < row.len() {
                col_sizes.resize(row.len(), 0);
            }

            for (i, col) in row.iter().enumerate() {
                col_sizes[i] = core::cmp::max(col_sizes[i], count_visible_chars_in_string(&col));
            }
        }

        const SPACING: usize = 2;

        for row in &self.rows {
            let mut line = String::new();

            for (i, col) in row.iter().enumerate() {
                // TODO: Cache this when inserting.
                let size = count_visible_chars_in_string(&col);
                
                line.push_str(&col);

                for _ in 0..(col_sizes[i] - size + SPACING) {
                    line.push(' ');
                }
            }

            println!("{}", line);
        }
    }
}

fn count_visible_chars_in_string(mut s: &str) -> usize {
    let mut n = 0;

    while !s.is_empty() {
        if let Some(rest) = s.strip_prefix("\x1B]8;;") {
            if let Some((_, rest)) = rest.split_once("\x1B\\") {
                s = rest;
                continue;
            }
        }
        
        let mut chars = s.chars();
        chars.next();
        s = chars.as_str();
        n += 1;
    }

    n
}


impl<'a> TerminalTableRowBuilder<'a> {
    pub fn col<T: std::convert::Into<String>>(&mut self, s: T) -> &mut Self {
        // TODO: Replace tabs with spaces in the string since they are not monospace.

        self.row.push(s.into());
        self
    }
}