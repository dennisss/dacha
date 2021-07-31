

mod child;
mod fd;
mod logging;

use std::sync::Arc;
use std::path::{Path, PathBuf};
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};

use common::errors::*;
use common::async_std::{fs, task};
use common::async_std::fs::File;
use common::async_std::sync::Mutex;
use common::async_std::channel;
use common::task::ChildTask;
use nix::sched::CloneFlags;
use nix::sys::signal::{Signal, SigHandler, signal};
use nix::sys::wait::WaitPidFlag;
use nix::unistd::Pid;

use crate::proto::log::*;
use crate::proto::config::*;
use crate::runtime::fd::*;
use crate::runtime::child::*;
use crate::runtime::logging::*;



/// Directory used to per-container instance data.
///
/// Under this directory, files will be stored as follows:
/// - '{container-id}/root' : Directory used as the root fs of the container.
/// - '{container-id}/log' : Append-only LevelDB log file containing LogEntry protos.
const RUN_DATA_DIR: &'static str = "/opt/dacha/container/run";

/// Stored a boolean value representing whether or not a ContainerRuntime instance has been created
/// in the current process.
///
/// Used to prevent multiple instances from being started.
static INSTANCE_LOCK: AtomicBool = AtomicBool::new(false);


static mut SIGCHLD_CHANNEL: Option<(channel::Sender<()>, channel::Receiver<()>)> = None;
static SIGCHLD_CHANNEL_INIT: Once = Once::new();

fn get_sigchld_channel() -> &'static (channel::Sender<()>, channel::Receiver<()>) {
    unsafe {
        SIGCHLD_CHANNEL_INIT.call_once(|| {
            SIGCHLD_CHANNEL = Some(channel::bounded(1));
        });

        SIGCHLD_CHANNEL.as_ref().unwrap()
    }
}

extern fn signal_handler_sigchld(signal: libc::c_int) {
    let _ = get_sigchld_channel().0.try_send(());
}

struct Container {
    metadata: ContainerMetadata,

    /// Directory where we store the container root and other files such as logs.
    directory: PathBuf,

    pid: Pid,

    // TODO: Make sure this is cleaned up
    waiter_task: task::JoinHandle<()>,

    /// Used to notify the waiter_task when the process associated with this container
    /// has exited.
    event_sender: channel::Sender<ContainerStatus>,
}

struct ContainerWaiter {
    container_id: String,
    container_dir: PathBuf,
    stdout: File,
    stderr: File,
    event_receiver: channel::Receiver<ContainerStatus>
}

// We want to be able to support subscribing to any events that occur for a 

pub struct ContainerRuntime {
    containers: Mutex<Vec<Container>>,

    /// 
    ///
    /// TODO: Convert to HashSet based listeners as we never need to deliver a single container id
    /// if there already is an enqueued event for it. 
    event_listeners: Mutex<Vec<channel::Sender<String>>>
}

impl Drop for ContainerRuntime {
    fn drop(&mut self) {
        // Relinquish our exclusive lock.
        INSTANCE_LOCK.swap(false, Ordering::SeqCst);
    }
}

impl ContainerRuntime {

    /// Creates a new runtime starting any background tasks needed.
    ///
    /// After creating a ContainerRuntime, the user wait on run() while there are running containers.
    ///
    /// NOTE: Only one instance of the ContainerRuntime is allowed to exist in the same process.
    pub async fn create() -> Result<Arc<Self>> {
        if INSTANCE_LOCK.swap(true, Ordering::SeqCst) {
            return Err(err_msg("ContainerRuntime instance already exists"));
        }

        // TODO: Verify that there is only one copy of this handler.
        let handler = SigHandler::Handler(signal_handler_sigchld);
        unsafe { signal(Signal::SIGCHLD, handler) }?;

        
        Ok(Arc::new(Self {
            containers: Mutex::new(vec![]),
            event_listeners: Mutex::new(vec![])
        }))
    }

    /// Runs the processing loop of the runtime. This must be continously polled while the ContainerRuntime
    /// is in use.
    pub async fn run(self: Arc<Self>) -> Result<()> {
        loop {
            get_sigchld_channel().1.recv().await?;

            loop {
                let e = match nix::sys::wait::waitpid(None, Some(WaitPidFlag::WUNTRACED | WaitPidFlag::WNOHANG)) {
                    Ok(e) => e,
                    Err(nix::Error::Sys(nix::errno::Errno::ECHILD)) => {
                        // This means that we don't have any more children.
                        // Break so that we wait for the next SIGCHLD signal to reap more.
                        break;
                    }
                    Err(e) => { return Err(e.into()); }
                };

                let containers = self.containers.lock().await;

                match e {
                    nix::sys::wait::WaitStatus::Exited(pid, exit_code) => {
                        let container = containers.iter().find(|c| c.pid == pid).unwrap();

                        let mut status = ContainerStatus::default();
                        status.set_exit_code(exit_code);

                        let _ = container.event_sender.try_send(status);
                    },
                    nix::sys::wait::WaitStatus::Signaled(pid, signal, _) => {
                        let container = containers.iter().find(|c| c.pid == pid).unwrap();

                        let mut status = ContainerStatus::default();
                        status.set_killed_signal(signal.as_str());

                        let _ = container.event_sender.try_send(status);
                    },
                    nix::sys::wait::WaitStatus::PtraceEvent(pid, _, _) |
                    nix::sys::wait::WaitStatus::PtraceSyscall(pid) |
                    nix::sys::wait::WaitStatus::Stopped(pid, _) => {
                        nix::sys::signal::kill(pid, Signal::SIGKILL)?;
                    },
                    nix::sys::wait::WaitStatus::Continued(_) => {},
                    nix::sys::wait::WaitStatus::StillAlive => { break; },
                }
            }

        }
    }

    pub async fn add_event_listener(&self) -> channel::Receiver<String> {
        let (sender, receiver) = channel::unbounded();

        let mut listeners = self.event_listeners.lock().await;
        listeners.push(sender);

        receiver
    }

    pub async fn get_container(&self, container_id: &str) -> Option<ContainerMetadata> {
        let containers = self.containers.lock().await;
        containers.iter().find(|c| c.metadata.id() == container_id)
            .map(|c| c.metadata.clone())
    }

    pub async fn list_containers(&self) -> Vec<ContainerMetadata> {
        let containers = self.containers.lock().await;

        let mut output = vec![];
        output.reserve_exact(containers.len());
        for container in containers.iter() {
            output.push(container.metadata.clone());
        }

        output
    }

    /// Starts a container returning the id of that container.
    pub async fn start_container(self: &Arc<Self>, container_config: &ContainerConfig) -> Result<String> {
        let mut container_id = vec![0u8; 16];
        crypto::random::secure_random_bytes(&mut &mut container_id).await?;
    
        let container_id = common::hex::encode(&container_id);
    
        // TODO: Also lock down permissions on this dir.
        let container_dir = Path::new(RUN_DATA_DIR).join(&container_id);
        fs::create_dir_all(&container_dir).await?;
        
        let mut stack = vec![0u8; 1024*1024*1]; // 1MB

        // NOTE: We use 'clone()' instead of 'fork()' to immediately put the sub-process into a new PID namespace
        // ('unshare()' requires an extra fork for that)
    
        let (stdout_read, stdout_write) = FileReference::pipe()?;
        let (stderr_read, stderr_write) = FileReference::pipe()?;

        // TODO: Do something with stdin.
        let mut file_mapping = FileMapping::default();
        file_mapping
            .add(STDOUT, stdout_write)
            .add(STDERR, stderr_write);

        // TODO: Uncomment these?
        // When used in the parent, we don't want them to be blocking.
        // fcntl(stdout_read.as_raw_fd(), FcntlArg::F_SETFL(OFlag::O_NONBLOCK))?;
        // fcntl(stderr_read.as_raw_fd(), FcntlArg::F_SETFL(OFlag::O_NONBLOCK))?;


        // NOTE: We must lock this before clone() and until we insert the new container to ensure
        // that waitpid() doesn't return before the container is in the list.
        let mut containers = self.containers.lock().await;

        // TODO: CLONE_INTO_CGROUP
        // TODO: Can memory (e.g. keys from the parent progress be read after the fork and do we need security against this?).
        let pid = nix::sched::clone(Box::new(|| {
            run_child_process(&container_config, &container_dir, &file_mapping)
        }), &mut stack, CloneFlags::CLONE_NEWPID | CloneFlags::CLONE_NEWNS, Some(libc::SIGCHLD))?;

        let (event_sender, event_receiver) = channel::bounded(1);

        let waiter_task = task::spawn(self.clone().container_waiter(ContainerWaiter {
            container_id: container_id.clone(), container_dir: container_dir.clone(),
            stdout: stdout_read.open()?.into(), stderr: stderr_read.open()?.into(), event_receiver
        }));

        let mut meta = ContainerMetadata::default();
        meta.set_id(container_id.clone());
        meta.set_state(ContainerState::Running);

        containers.push(Container {
            metadata: meta,
            directory: container_dir.clone(),
            pid,
            waiter_task,
            event_sender
        });

        drop(containers);

        // Drop in the parent process.
        drop(file_mapping);

        Ok(container_id)
    }

    pub async fn kill_container(&self, container_id: &str, signal: Signal) -> Result<()> {
        let containers = self.containers.lock().await;
        let container = match containers.iter().find(|c| c.metadata.id() == container_id) {
            Some(c) => c,
            None => {
                return Err(err_msg("Container not found"));
            }
        };

        // TODO: Check that the container is still running.
        // If it is still being created, we may need to take special action to kill it.

        // TODO: Should I ignore ESRCH?
        nix::sys::signal::kill(container.pid, signal)?;
        Ok(())
    }

    pub async fn open_log(&self, container_id: &str) -> Result<FileLogReader> {
        let containers = self.containers.lock().await;
        let container = match containers.iter().find(|c| c.metadata.id() == container_id) {
            Some(c) => c,
            None => {
                return Err(err_msg("Container not found"));
            }
        };

        let log_path = container.directory.join("log");
        drop(containers);

        // TODO: For this to work we need to ensure that the log file is synchronously created before
        // we return the container id to the person that reuqested it.
        FileLogReader::open(&log_path).await
    }

    async fn write_log(file: File, log_writer: Arc<FileLogWriter>, stream: LogStream) {
        if let Err(e) = log_writer.write_stream(file, stream).await {
            eprintln!("Log writer failed: {:?}", e);
        }
    }

    async fn container_waiter(self: Arc<Self>, input: ContainerWaiter) {
        let res = self.container_waiter_inner(input).await;

        // TODO: Some types of errors should take down the entire server and others should be
        // isolated to invalidating this container.
        if let Err(e) = res {
            eprintln!("Container waiter error: {:?}", e);
        }
    }

    async fn container_waiter_inner(self: Arc<Self>, input: ContainerWaiter) -> Result<()> {
        let log_writer = Arc::new(FileLogWriter::create(
            &input.container_dir.join("log")).await?);

        let stdout_task = ChildTask::spawn(Self::write_log(
            input.stdout, log_writer.clone(), LogStream::STDOUT));
        
        let stderr_task = ChildTask::spawn(Self::write_log(
            input.stderr, log_writer.clone(), LogStream::STDERR));

        let status = input.event_receiver.recv().await?;
        
        stdout_task.join().await;
        stderr_task.join().await;

        // TODO: Flush the log? (fsync, etc.)

        let container_id = input.container_id.as_str();

        let mut containers = self.containers.lock().await;
        let container = containers.iter_mut()
            .find(|c| c.metadata.id() == container_id).unwrap();

        container.metadata.set_state(ContainerState::Stopped);
        container.metadata.set_status(status);

        {
            let mut listeners = self.event_listeners.lock().await;
            let mut i = 0;
            while i < listeners.len() {
                if let Err(_) = listeners[i].send(container_id.to_string()).await {
                    listeners.swap_remove(i);
                } else {
                    i += 1;
                }
            }
        }

        Ok(())
    }


}

