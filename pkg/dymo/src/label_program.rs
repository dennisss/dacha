use common::errors::*;

const START_SEQUENCE: &[u8] = &[0u8; 8];

const INSTRUCTION_START: u8 = 0x1b;

const MAX_BYTES_PER_LINE: usize = 8;

/// Size of the set_skip_bytes and set_bytes_per_line instructions.
const INSTRUCTION_SIZE: usize = 3;

pub struct LabelProgramBuilder {
    program: Vec<u8>,
    skip_bytes: usize,
    bytes_per_line: usize,
}

impl LabelProgramBuilder {
    pub fn new() -> Self {
        let mut program = vec![];
        program.extend_from_slice(START_SEQUENCE);

        Self {
            program,
            skip_bytes: 0,
            bytes_per_line: 0,
        }
    }

    // We assume all input lines are of size MAX_BYTES_PER_LIN
    pub fn compile_lines(lines: &[Vec<u8>]) -> Result<Vec<u8>> {
        let mut program = Self::new();
        program.set_color(0);

        for line in lines {
            if line.len() > MAX_BYTES_PER_LINE {
                return Err(err_msg("Expected lines to be full"));
            }

            let range = LineRange::from(&line);

            if range.start_index < program.skip_bytes()
                || range.start_index > program.skip_bytes() + INSTRUCTION_SIZE
            {
                program.set_skip_bytes(range.start_index)?;
            }

            let target_len = range.end_index - program.skip_bytes();
            if target_len > program.bytes_per_line()
                || target_len + INSTRUCTION_SIZE < program.bytes_per_line()
            {
                program.set_bytes_per_line(target_len)?;
            }

            program.print_line(
                &line[program.skip_bytes()..(program.skip_bytes() + program.bytes_per_line())],
            )?;
        }

        Ok(program.finish())
    }

    pub fn skip_bytes(&self) -> usize {
        self.skip_bytes
    }

    pub fn set_skip_bytes(&mut self, num: usize) -> Result<()> {
        if num == self.skip_bytes {
            return Ok(());
        }

        if num > MAX_BYTES_PER_LINE {
            return Err(err_msg("New skip_bytes value larger than max bytes"));
        }

        self.skip_bytes = num;
        self.program
            .extend_from_slice(&[INSTRUCTION_START, b'B', num as u8]);
        Ok(())
    }

    pub fn set_color(&mut self, color: u8) {
        self.program
            .extend_from_slice(&[INSTRUCTION_START, b'C', color]);
    }

    fn bytes_per_line(&self) -> usize {
        self.bytes_per_line
    }

    pub fn set_bytes_per_line(&mut self, num: usize) -> Result<()> {
        if num == self.bytes_per_line {
            return Ok(());
        }

        if num > MAX_BYTES_PER_LINE {
            return Err(err_msg("New bytes_per_line larger than max bytes"));
        }

        self.bytes_per_line = num;
        self.program
            .extend_from_slice(&[INSTRUCTION_START, b'D', num as u8]);
        Ok(())
    }

    pub fn print_line(&mut self, data: &[u8]) -> Result<()> {
        if self.bytes_per_line + self.skip_bytes > MAX_BYTES_PER_LINE {
            return Err(err_msg("bytes_per_line + line_size > max bytes"));
        }

        if data.len() != self.bytes_per_line {
            return Err(err_msg("Incorrect line data size"));
        }

        self.program.push(0x16);
        self.program.extend_from_slice(data);

        Ok(())
    }

    pub fn finish(self) -> Vec<u8> {
        self.program
    }
}

struct LineRange {
    start_index: usize,
    end_index: usize,
}

impl LineRange {
    fn from(line: &[u8]) -> Self {
        let mut end_index = line.len();
        while end_index > 0 && line[end_index - 1] == 0 {
            end_index -= 1;
        }

        let mut start_index = 0;
        while start_index < end_index && line[start_index] == 0 {
            start_index += 1;
        }

        LineRange {
            start_index,
            end_index,
        }
    }
}
