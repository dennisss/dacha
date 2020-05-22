// Virtual machine for executing TrueType instructions.

use common::errors::*;

pub fn execute(inst_stream: &[u8]) -> Result<()> {
    let mut stack = Vec::<u32>::new();

    let mut pc = 0;
    while pc < inst_stream.len() {
        let opcode = inst_stream[pc];
        pc += 1;
    }

    Ok(())
}
