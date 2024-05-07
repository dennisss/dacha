//! This module contains a VM implementation of regular expression matching.
//! This is largely based on the design of RE2 / https://swtch.com/~rsc/regexp/regexp2.html

// TODO: Eventually make this private.
pub mod instruction;

mod compiler;
mod executor;
pub mod instance;

#[cfg(test)]
mod tests {
    use super::*;

    use super::compiler::*;
    use super::executor::*;
    use super::instruction::*;

    use common::errors::*;

    use crate::regexp::node::*;

    // Helper for running a program through an executor.
    fn run(prog: &[Instruction], inputs: &[u8]) -> Option<Vec<Option<usize>>> {
        let mut exec = Executor::new(ReferencedProgram::new(prog));
        exec.run(inputs, 0).map(|v| v.list)
    }

    #[test]
    fn vm_run_test() {
        let prog: &[Instruction] = &[
            Instruction::Char('a' as u32),
            Instruction::Char('b' as u32),
            Instruction::Char('c' as u32),
            Instruction::Match,
        ];

        assert_eq!(run(&prog, b"abb"), None);
        assert_eq!(run(&prog, b"bbc"), None);
        assert_eq!(run(&prog, b"abc"), Some(vec![]));

        // 'a|b'
        let prog: &[Instruction] = &[
            Instruction::Split(1, 3),      // 0
            Instruction::Char('a' as u32), // 1
            Instruction::Jump(4),          // 2
            Instruction::Char('b' as u32), // 3
            Instruction::Match,            // 4
        ];

        assert_eq!(run(&prog, b"a"), Some(vec![]));
        assert_eq!(run(&prog, b"b"), Some(vec![]));
        assert_eq!(run(&prog, b"c"), None);
    }

    #[test]
    fn vm_compile_test() -> Result<()> {
        let node = RegExpNode::parse("^a(b|c)d$")?;

        let prog = Compiler::compile(node.as_ref())?;

        println!("{}", prog.assembly());

        assert_eq!(run(&prog.program, b"abd"), Some(vec![Some(1), Some(2)]));
        assert_eq!(run(&prog.program, b"ad"), None);
        assert_eq!(run(&prog.program, b""), None);
        assert_eq!(run(&prog.program, b"acd"), Some(vec![Some(1), Some(2)]));
        assert_eq!(run(&prog.program, b"acdd"), None);

        Ok(())
    }

    #[test]
    fn vm_optimize_test() -> Result<()> {
        let node = RegExpNode::parse(".*(a)")?;

        let prog = Compiler::compile(node.as_ref())?;

        println!("{}", prog.assembly());

        assert_eq!(run(&prog.program, b""), None);
        assert_eq!(run(&prog.program, b"b"), None);
        assert_eq!(run(&prog.program, b"a"), Some(vec![Some(0), Some(1)]));
        assert_eq!(run(&prog.program, b"ax"), Some(vec![Some(0), Some(1)]));
        assert_eq!(run(&prog.program, b"hello a"), Some(vec![Some(6), Some(7)]));

        println!(
            "SIZE: {}",
            std::mem::size_of::<Instruction>() * prog.program.len()
        );

        Ok(())
    }

    #[test]
    fn vm_greedy_test() -> Result<()> {
        let re = instance::RegExp::new("ba+")?;

        let input = "axbaaaa";

        let m = re.exec(input).unwrap();

        assert_eq!(m.group_str(0).unwrap()?, "baaaa");

        Ok(())
    }

    #[test]
    fn vm_compile_range_test() -> Result<()> {
        let node = RegExpNode::parse("[a-b]")?;

        let prog = Compiler::compile(node.as_ref())?;

        assert_eq!(
            prog.program.instructions(),
            &[
                Instruction::Range {
                    start: 'a' as u32,
                    end: 'c' as u32
                },
                Instruction::Match
            ]
        );

        Ok(())
    }

    // TODO: We don't want '.' to match the empty string (given that it will
    // match the start marker)

    // All user provided regular expressions will be executed as if they are
    // actually ".*R" where "R" is the user input expression.

    // TODO: By default, "." shouldn't be able to match "\n"?
}
