use core::marker::PhantomData;

use alloc::borrow::ToOwned;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use common::errors::*;
use common::io::Readable;
use file::LocalFile;

const MAX_ROW_SIZE: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Errable, PartialEq)]
#[cfg_attr(feature = "std", derive(Fail))]
#[repr(u32)]
pub enum CSVError {
    IncompleteRow,
    RowExceedsMaxLength,
    InvalidUTF8,
}

impl core::fmt::Display for CSVError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub type CSVResult<T> = core::result::Result<T, CSVError>;

/// NOTE: This assumes that the input consists of UTF-8 characters.
pub struct CSVParser {
    /// Concatenated data of all fields in the current row being parsed.
    data: Vec<u8>,

    /// Offset in 'data' at which each field in the row ends (excluding the last
    /// field which ends at data.len()).
    field_ends: Vec<usize>,

    /// Parsing grammar state definiting which tokens are expected next.
    state: State,

    /// If true, we have decoded a complete line.
    done_row: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum State {
    /// Expecting regular characters to be appended to the last field as data.
    Normal,

    /// We encountered a quote so we are now accepting data in an escaped state.
    FirstQuote,

    /// We saw a quote while in the escaped (FirstQuote) state. We need to
    /// observe another character to determine if this is the end of the escaped
    /// section or an escaped quote.
    SecondQuote,

    /// Saw a '\r' character. Waiting for a '\n' character.
    CarriageReturn,
}

impl CSVParser {
    pub fn parse_all<I: AsRef<[u8]>>(input: I) -> CSVResult<Vec<Vec<String>>> {
        let mut out = vec![];

        let mut parser = Self::new();
        let mut input = input.as_ref();
        while !input.is_empty() {
            let i = parser.parse(input, true)?;
            input = &input[i..];

            if parser.done_row() {
                let mut fields = vec![];
                for i in 0..parser.num_fields() {
                    fields.push(parser.field(i)?.to_owned());
                }

                out.push(fields);
            }
        }

        Ok(out)
    }

    pub fn new() -> Self {
        Self {
            data: vec![],
            field_ends: vec![],
            state: State::Normal,
            done_row: false,
        }
    }

    pub fn parse(&mut self, mut input: &[u8], end_of_input: bool) -> CSVResult<usize> {
        let mut n = 0;
        // Loop at least once to
        loop {
            let i = self.parse_row(input, end_of_input)?;
            n += i;
            input = &input[i..];

            // Skip empty lines.
            if self.done_row && self.data.is_empty() && self.field_ends.is_empty() {
                self.clear_row();
            }

            if input.is_empty() || self.done_row() {
                break;
            }
        }

        Ok(n)
    }

    fn parse_row(&mut self, input: &[u8], end_of_input: bool) -> CSVResult<usize> {
        if self.done_row {
            self.clear_row();
        }

        // Number of input bytes which were consumed.
        let mut i = 0;

        while i < input.len() {
            let b = input[i];

            if self.data.len() > MAX_ROW_SIZE {
                return Err(CSVError::RowExceedsMaxLength);
            }

            match self.state {
                State::Normal => {
                    if b == b'"' {
                        self.state = State::FirstQuote;
                    } else if b == b',' {
                        self.field_ends.push(self.data.len());
                    } else if b == b'\r' {
                        self.state = State::CarriageReturn;
                    } else if b == b'\n' {
                        self.done_row = true;
                        i += 1;
                        break;
                    } else {
                        self.data.push(b);
                    }
                }
                State::FirstQuote => {
                    if b == b'"' {
                        self.state = State::SecondQuote;
                    } else {
                        self.data.push(b);
                    }
                }
                State::SecondQuote => {
                    if b == b'"' {
                        // Escaped quota
                        self.data.push(b);
                        self.state = State::FirstQuote;
                    } else {
                        self.state = State::Normal;
                        // Re-process this byte using the Normal rules.
                        continue;
                    }
                }
                State::CarriageReturn => {
                    if b == b'\n' {
                        // Line ending was '\r\n'
                        self.done_row = true;
                        i += 1;
                        break;
                    } else {
                        // Line ending was just '\r' without a '\n'
                        // Don't consume the next byte.
                        break;
                    }
                }
            }

            i += 1;
        }

        if i == input.len() && end_of_input && !self.done_row {
            if self.state == State::FirstQuote {
                return Err(CSVError::IncompleteRow);
            }

            // Accept lines without a proper line ending.
            self.done_row = true;
        }

        Ok(i)
    }

    fn clear_row(&mut self) {
        self.done_row = false;
        self.data.clear();
        self.field_ends.clear();
        self.state = State::Normal;
    }

    /// If true, a single row is internally buffered and can be read out.
    /// Note that we skip empty lines.
    pub fn done_row(&self) -> bool {
        self.done_row
    }

    /// If true, a partial row has been parsed and additional data is required
    /// to complete it.
    pub fn incomplete_row(&self) -> bool {
        if self.done_row {
            return false;
        }

        !self.data.is_empty() || !self.field_ends.is_empty() || self.state == State::FirstQuote
    }

    pub fn num_fields(&self) -> usize {
        self.field_ends.len() + 1
    }

    pub fn field(&self, index: usize) -> CSVResult<&str> {
        let start = if index == 0 {
            0
        } else {
            self.field_ends[index - 1]
        };
        let end = self
            .field_ends
            .get(index)
            .cloned()
            .unwrap_or(self.data.len());
        core::str::from_utf8(&self.data[start..end]).map_err(|_| CSVError::InvalidUTF8)
    }
}

/// CSV Reader. Assumes that the input file is UTF-8 encoded.
///
/// TODO: Implement parallelized reading from a large CSV.
pub struct CSVReader {
    file: LocalFile,
    parser: CSVParser,
    buffer: Vec<u8>,
    buffer_offset: usize,
    buffer_length: usize,
}

impl CSVReader {
    pub fn new(file: LocalFile) -> Self {
        Self {
            file,
            parser: CSVParser::new(),

            // TODO: this is a fairly common pattern. Encapsulate it and use it for stuff like line
            // matchers.
            buffer: vec![0u8; 4096],
            buffer_offset: 0,
            buffer_length: 0,
        }
    }

    /// Reads a single row from the CSV.
    pub async fn read(&mut self) -> Result<Option<&CSVParser>> {
        loop {
            if self.buffer_offset == self.buffer_length {
                let n = self.file.read(&mut self.buffer).await?;
                self.buffer_offset = 0;
                self.buffer_length = n;
            }

            let at_eof = self.buffer_length == 0;

            let n = self
                .parser
                .parse(&self.buffer[self.buffer_offset..self.buffer_length], at_eof)?;

            self.buffer_offset += n;

            if self.parser.done_row() {
                return Ok(Some(&self.parser));
            } else if at_eof {
                return Ok(None);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use core::num;

    use file::project_path;

    use super::*;

    #[test]
    fn csv_parsing_test() {
        assert_eq!(CSVParser::parse_all(""), Ok(vec![]));
        assert_eq!(CSVParser::parse_all("\r"), Ok(vec![]));
        assert_eq!(CSVParser::parse_all("\r\n"), Ok(vec![]));
        assert_eq!(CSVParser::parse_all("\n"), Ok(vec![]));

        assert_eq!(
            CSVParser::parse_all("hello"),
            Ok(vec![vec!["hello".to_string()]])
        );

        assert_eq!(
            CSVParser::parse_all("hello,world"),
            Ok(vec![vec!["hello".to_string(), "world".to_string()]])
        );

        assert_eq!(
            CSVParser::parse_all("hello,world\n"),
            Ok(vec![vec!["hello".to_string(), "world".to_string()]])
        );

        assert_eq!(
            CSVParser::parse_all("hello,world\n\n\n3,4\n5,6,7"),
            Ok(vec![
                vec!["hello".to_string(), "world".to_string()],
                vec!["3".to_string(), "4".to_string()],
                vec!["5".to_string(), "6".to_string(), "7".to_string()]
            ])
        );

        assert_eq!(CSVParser::parse_all("\""), Err(CSVError::IncompleteRow));

        assert_eq!(
            CSVParser::parse_all("\"hello,wor\"\"ld\",app\"l\"es\na,b"),
            Ok(vec![
                vec!["hello,wor\"ld".to_string(), "apples".to_string()],
                vec!["a".to_string(), "b".to_string()],
            ])
        );
    }

    #[testcase]
    async fn csv_iris_parsing() -> Result<()> {
        let mut reader = CSVReader::new(file::LocalFile::open(project_path!(
            "third_party/datasets/iris/iris.data"
        ))?);

        let p = reader.read().await?.unwrap();
        assert_eq!(p.num_fields(), 5);
        assert_eq!(p.field(0), Ok("5.1"));
        assert_eq!(p.field(1), Ok("3.5"));
        assert_eq!(p.field(2), Ok("1.4"));
        assert_eq!(p.field(3), Ok("0.2"));
        assert_eq!(p.field(4), Ok("Iris-setosa"));

        let p = reader.read().await?.unwrap();
        assert_eq!(p.num_fields(), 5);
        assert_eq!(p.field(0), Ok("4.9"));
        assert_eq!(p.field(1), Ok("3.0"));
        assert_eq!(p.field(2), Ok("1.4"));
        assert_eq!(p.field(3), Ok("0.2"));
        assert_eq!(p.field(4), Ok("Iris-setosa"));

        let mut num_remaining = 0;
        while reader.read().await?.is_some() {
            num_remaining += 1;
        }

        assert_eq!(num_remaining, 148); // 150 total if including the two we already read.

        // TODO: Write some test to verify which record is returned last in the
        // CSVReader.

        Ok(())
    }
}
