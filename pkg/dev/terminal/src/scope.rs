use base_error::*;
use nix::{
    sys::termios::{Termios, tcgetattr, tcsetattr, ControlFlags, InputFlags, LocalFlags, OutputFlags},
    unistd::isatty,
};

pub struct TermiosScope {
    fd: i32,
    old_value: Termios,
}

impl Drop for TermiosScope {
    fn drop(&mut self) {
        let _ = tcsetattr(self.fd, nix::sys::termios::SetArg::TCSAFLUSH, &self.old_value);
    }
}

impl TermiosScope {
    pub fn no_echo_stdin() -> Result<Self> {
        let mut termios = tcgetattr(0)?;
        let old_value = termios.clone();
        termios.local_flags.remove(LocalFlags::ECHO);
        tcsetattr(0, nix::sys::termios::SetArg::TCSAFLUSH, &termios)?;
        Ok(Self { fd: 0, old_value })
    }
}
