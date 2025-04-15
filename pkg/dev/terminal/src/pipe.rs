use base_error::*;
use nix::{
    sys::termios::{tcgetattr, tcsetattr, ControlFlags, InputFlags, LocalFlags, OutputFlags},
    unistd::isatty,
};
use common::io::{Readable, Writeable, SharedWriteable};
use nix::pty::posix_openpt;
use nix::fcntl::OFlag;
use nix::unistd::{dup2, dup};
use sys::OpenFileDescriptor;
use executor::channel;
use executor::channel::spsc;
use executor::bundle::TaskResultBundle;

/*
TODO: stdin and stdout will probably be the same underlying descriptor so unsure that termios scopes are resotred consistently.
*/

/*
Issues:
- setsid() 
- See also https://github.com/Yelp/dumb-init/pull/122 and https://github.com/Yelp/dumb-init/issues/51
- Idelaly Detach, then setsid(), then attach, then 


Basically this is for 
*/
pub async fn run_terminal_client() -> Result<()> {

    let mut bundle = TaskResultBundle::new();

    let (child, pty_master) = start_child_process().await?;
    let (mut pty_reader, mut pty_writer) = pty_master.split();

    let (stdin_sender, mut stdin_receiver) = spsc::bounded(10);
    bundle.add("StdinReader", read_local_stdin(stdin_sender));
    bundle.add("StdinWriter", async move {
        loop {
            let data = stdin_receiver.recv().await?;
            pty_writer.write_all(&data).await?;
        }
    });

    let mut stdout = file::Stdout::get();

    // let (stdout_sender, stdout_receiver) = spsc::bounded(10);

    /*
    
    while let Some(entry) = log_stream.recv().await {
        // TODO: If we are not in terminal mode, restrict ourselves to only writing out
        // characters that are in the ASCII visible range (so that we can't
        // effect the terminal with escape codes).

        
    }
    */

    bundle.add("StdoutReader", async move {
        let mut buf = [0u8; 512];
        loop {
            let n = pty_reader.read(&mut buf[..]).await?;
            if n == 0 {
                println!("<EOO>");
                return Ok(());
            }

            // stdout_sender.

            stdout.write_all(&buf[0..n]).await?;
            stdout.flush().await?;
        }
    });


    bundle.join().await?;

    Ok(())
}

async fn read_local_stdin(mut output: spsc::Sender<Vec<u8>>) -> Result<()> {

    if !isatty(0)? {
        return Err(err_msg("Expected stdin to be a tty"));
    }

    // A good explanation of these flags is present in:
    // https://viewsourcecode.org/snaptoken/kilo/02.enteringRawMode.html#disable-raw-mode-at-exit

    let mut termios = tcgetattr(0)?;
    // Disable echoing of every input character to the output.
    termios.local_flags.remove(LocalFlags::ECHO);
    // Disable canonical mode: meaning we'll read bytes at a time instead of only
    // reading once an entire line was written.
    termios.local_flags.remove(LocalFlags::ICANON);
    // Disable receiving a signal for Ctrl-C and Ctrl-Z.
    // termios.local_flags.remove(LocalFlags::ISIG);
    // Disable Ctrl-S and Ctrl-Q.
    termios.input_flags.remove(InputFlags::IXON);
    // Disable Ctrl-V.
    termios.local_flags.remove(LocalFlags::IEXTEN);

    termios.input_flags.remove(InputFlags::ICRNL);
    termios.output_flags.remove(OutputFlags::OPOST);

    termios
        .input_flags
        .remove(InputFlags::BRKINT | InputFlags::INPCK | InputFlags::ISTRIP);
    termios.control_flags |= ControlFlags::CS8;

    tcsetattr(0, nix::sys::termios::SetArg::TCSAFLUSH, &termios)?;

    // TODO: When we create the tty on the server, do we need to explicitly enable
    // all of the above flags.

    let mut stdin = file::Stdin::get();

    loop {
        let mut data = [0u8; 512];

        let n = stdin.read(&mut data).await.expect("Stdin Read failed");
        if n == 0 {
            println!("EOI");
            break;
        }

        output.send(data[0..n].to_vec()).await?;

        // println!("INPUT: \"{}\"", base_util::format::format_bytes(&data[0..n]));
    }

    Ok(())
}



use std::process::{Stdio, Child};
use nix::sys::stat::Mode;
use std::os::fd::{FromRawFd, IntoRawFd};
use executor::FileHandle;

pub async fn start_child_process() -> Result<(Child, PTYMaster)> {
    // Opening '/dev/ptmx' to obtain a pseudoterminal master fd.
    let term_primary = posix_openpt(OFlag::O_RDWR | OFlag::O_CLOEXEC)?;
    println!("Master: {:?}", term_primary);

    // Get the path to the slave device corresponding to the master fd.
    let term_secondary_path: String = unsafe { nix::pty::ptsname(&term_primary) }?;
    println!("Slave: {}", term_secondary_path);

    // After this, the slave device will be openable and will be owned by the
    // real UID of the current process.
    nix::pty::grantpt(&term_primary)?;
    nix::pty::unlockpt(&term_primary)?;

    let term_primary = OpenFileDescriptor::new(term_primary.into_raw_fd());

    // TODO: Use 'sys' code for this.
    let term_secondary = OpenFileDescriptor::new(nix::fcntl::open(
        std::path::Path::new(&term_secondary_path),
        OFlag::O_RDWR | OFlag::O_CLOEXEC,
        Mode::empty(),
    )?);

    // TODO: Locally close term_secondary (ensure we also do this in the container code)..

    fn make_stdio(fd: &OpenFileDescriptor) -> Stdio {
        let fd = dup(**fd).unwrap();
        unsafe { Stdio::from_raw_fd(fd) }
    }

    let child = std::process::Command::new("/bin/bash")
        .env_clear()
        .stdin(make_stdio(&term_secondary))
        .stdout(make_stdio(&term_secondary))
        .stderr(make_stdio(&term_secondary))
        .spawn()?;


    let pty_master = PTYMaster {
        file: FileHandle::new(term_primary, false)
    };

    Ok((child, pty_master))
}

pub struct PTYMaster {
    file: FileHandle,
}


impl PTYMaster {
    pub fn split(mut self) -> (Box<dyn Readable + Sync>, Box<dyn SharedWriteable>) {
        let reader = Box::new(Self {
            file: self.file.clone(),
        });

        (reader, Box::new(self))
    }
}

#[async_trait]
impl Readable for PTYMaster {
    async fn read(&mut self, output: &mut [u8]) -> Result<usize> {
        self.file.read(output).await
    }
}

#[async_trait]
impl Writeable for PTYMaster {
    async fn write(&mut self, data: &[u8]) -> Result<usize> {
        self.file.write(data).await
    }

    async fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}


