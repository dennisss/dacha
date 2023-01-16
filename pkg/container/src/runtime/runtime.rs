use std::io::Write;
use std::os::unix::prelude::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Once;
use std::time::Duration;

use common::errors::*;
use common::io::Writeable;
use crypto::random::SharedRng;
use executor::bundle::TaskResultBundle;
use executor::channel;
use executor::child_task::ChildTask;
use executor::signals::*;
use executor::sync::Mutex;
use executor::JoinHandle;
use file::{LocalFile, LocalPath, LocalPathBuf};
use sys::{WaitOptions, WaitStatus};

use crate::proto::config::*;
use crate::proto::log::*;
use crate::runtime::child::*;
use crate::runtime::fd::*;
use crate::runtime::logging::*;
use crate::setup_socket::SetupSocket;

use super::cgroup::Cgroup;
use super::constants::{FINISHED_SETUP_BYTE, TERMINAL_FD_BYTE, USER_NS_SETUP_BYTE};

// Let's move this stuff into separate submodules

/// Represents whether or not a ContainerRuntime instance has been created in
/// the current process.
///
/// Used to prevent multiple instances from being started. We only support
/// having one instance per process because we can't multiplex SIGCHLD signals.
///
/// TODO: Instead
static INSTANCE_LOCK: AtomicBool = AtomicBool::new(false);

// We want to be able to support subscribing to any events that occur for a

pub struct ContainerRuntime {
    /// Directory used to per-container instance data.
    ///
    /// Under this directory, files will be stored as follows:
    /// - '{container-id}/root' : Directory used as the root fs of the
    ///   container.
    /// - '{container-id}/log' : Append-only LevelDB log file containing
    ///   LogEntry protos.
    run_dir: LocalPathBuf,

    /// Cgroup V2 file system root directory.
    ///
    /// We assume that the current user is the owner of this directory (e.g.
    /// systemd has granted us our own cgroup).
    ///
    /// We will create a directories named '{container-id}' under this directory
    /// for each container.
    cgroup_dir: LocalPathBuf,

    /// State associated with all started containers.
    /// Entries are only removed from this set when the user calls
    /// remove_container().
    containers: Mutex<Vec<Container>>,

    ///
    ///
    /// TODO: Convert to HashSet based listeners as we never need to deliver a
    /// single container id if there already is an enqueued event for it.
    event_listeners: Mutex<Vec<channel::Sender<String>>>,
}

struct Container {
    metadata: ContainerMetadata,

    /// Directory where we store the container root and other files such as
    /// logs.
    directory: LocalPathBuf,

    /// TODO: Clean this the backing directory for this eventually.
    cgroup: Cgroup,

    pid: sys::pid_t,

    /// If this container was
    stdin: Mutex<Option<LocalFile>>,

    // TODO: Make sure this is cleaned up
    waiter_task: Option<JoinHandle<()>>,

    /// Used to notify the waiter_task when the process associated with this
    /// container has exited.
    event_sender: channel::Sender<ContainerStatus>,
}

impl Container {
    fn metadata(&self) -> ContainerMetadata {
        let mut meta = self.metadata.clone();
        meta.resource_usage_mut()
            .set_cpu_fraction(self.cgroup.cpu_usage());
        meta.resource_usage_mut()
            .set_memory_bytes(self.cgroup.memory_usage());
        meta
    }
}

struct ContainerWaiter {
    container_id: String,
    container_dir: LocalPathBuf,
    // TODO: a LocalFile is unappropriate with a this as it is not seekable.
    output_streams: Vec<(LogStream, LocalFile)>,
    event_receiver: channel::Receiver<ContainerStatus>,
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
    /// After creating a ContainerRuntime, the user wait on run() while there
    /// are running containers.
    ///
    /// NOTE: Only one instance of the ContainerRuntime is allowed to exist in
    /// the same process.
    pub async fn create<P: AsRef<LocalPath>>(run_dir: P, cgroup_dir: &str) -> Result<Arc<Self>> {
        if INSTANCE_LOCK.swap(true, Ordering::SeqCst) {
            return Err(err_msg("ContainerRuntime instance already exists"));
        }

        Ok(Arc::new(Self {
            run_dir: run_dir.as_ref().to_owned(),
            cgroup_dir: LocalPathBuf::from(cgroup_dir),
            containers: Mutex::new(vec![]),
            event_listeners: Mutex::new(vec![]),
        }))
    }

    /// Runs the processing loop of the runtime. This must be continously polled
    /// while the ContainerRuntime is in use.
    pub async fn run(self: Arc<Self>) -> Result<()> {
        let mut bundle = TaskResultBundle::new();
        bundle.add(
            "cluster::ContainerRuntime::run_waitpid",
            Self::run_waitpid(self.clone()),
        );
        bundle.add(
            "cluster::ContainerRuntime::run_cgroup_monitor",
            Self::run_cgroup_monitor(self.clone()),
        );

        bundle.join().await
    }

    async fn run_waitpid(self: Arc<Self>) -> Result<()> {
        let mut sigchld_receiver = register_signal_handler(Signal::SIGCHLD)?;

        loop {
            sigchld_receiver.recv().await;

            loop {
                let e = match unsafe {
                    sys::waitpid(
                        -1, // TODO: Chance this to None
                        WaitOptions::WUNTRACED | WaitOptions::WNOHANG,
                    )
                } {
                    Ok(e) => e,
                    Err(sys::Errno::ECHILD) => {
                        // This means that we don't have any more children.
                        // Break so that we wait for the next SIGCHLD signal to reap more.
                        break;
                    }
                    Err(e) => {
                        return Err(e.into());
                    }
                };

                let containers = self.containers.lock().await;

                match e {
                    WaitStatus::Exited {
                        pid,
                        status: exit_code,
                    } => {
                        let container = containers.iter().find(|c| c.pid == pid).unwrap();

                        let mut status = ContainerStatus::default();
                        status.set_exit_code(exit_code);

                        let _ = container.event_sender.try_send(status);
                    }
                    WaitStatus::Signaled {
                        pid,
                        signal,
                        core_dumped,
                    } => {
                        let container = containers.iter().find(|c| c.pid == pid).unwrap();

                        let mut status = ContainerStatus::default();
                        status.set_killed_signal(signal.as_str().unwrap_or("UNKNOWN"));

                        let _ = container.event_sender.try_send(status);
                    }
                    // TODO: Kill these as well?
                    WaitStatus::Unknown { .. } => {}
                    // nix::sys::wait::WaitStatus::PtraceEvent(pid, _, _)
                    // | nix::sys::wait::WaitStatus::PtraceSyscall(pid)
                    WaitStatus::Stopped { pid, signal } => {
                        unsafe { sys::kill(pid, sys::Signal::SIGKILL)? };
                    }
                    WaitStatus::Continued { .. } => {}
                    WaitStatus::NoStatus => {
                        break;
                    }
                }
            }
        }
    }

    /// Task for periodically monitoring cgroup resource usage.
    async fn run_cgroup_monitor(self: Arc<Self>) -> Result<()> {
        loop {
            {
                let mut containers = self.containers.lock().await;

                for container in &mut containers[..] {
                    container.cgroup.collect_measurement().await?;
                }
            }

            executor::sleep(Duration::from_secs(1)).await?;
        }

        Ok(())
    }

    /// Registers and returns an event listener which will be notified when
    /// containers have entered the 'Stopped' state.
    ///
    /// Returns a receiver which will receive a container id once the associated
    /// container is stopped.
    pub async fn add_event_listener(&self) -> channel::Receiver<String> {
        let (sender, receiver) = channel::unbounded();

        let mut listeners = self.event_listeners.lock().await;
        listeners.push(sender);

        receiver
    }

    pub async fn get_container(&self, container_id: &str) -> Option<ContainerMetadata> {
        let containers = self.containers.lock().await;
        containers
            .iter()
            .find(|c| c.metadata.id() == container_id)
            .map(|c| c.metadata())
    }

    pub async fn list_containers(&self) -> Vec<ContainerMetadata> {
        let containers = self.containers.lock().await;

        let mut output = vec![];
        output.reserve_exact(containers.len());
        for container in containers.iter() {
            output.push(container.metadata());
        }

        output
    }

    /// Clears a container from in memory state.
    /// This is only allowed for containers which are currently stopped.
    ///
    /// NOTE: Artifacts such as logs in the file system will NOT be deleted.
    pub async fn remove_container(&self, container_id: &str) -> Result<()> {
        let mut containers = self.containers.lock().await;

        let container_index = containers
            .iter()
            .enumerate()
            .find(|(_, c)| c.metadata.id() == container_id)
            .ok_or_else(|| err_msg("Container being removed was not found"))?
            .0;

        if containers[container_index].metadata.state() != ContainerState::Stopped {
            return Err(err_msg("Not allowed to remove a running container"));
        }

        containers.swap_remove(container_index);

        Ok(())
    }

    /*
    There are OCI runtime tests in:
    - https://github.com/opencontainers/runtime-tools

    TODOs
    - idmapped mount
    - Mask the /proc

    TODO: If we ever give CAP_SYS_ADMIN to the container, then we'll need to isolate all of the /proc mounts into a separate
    parent namespace as we don't want to allow unmounting of the files.
    */

    /// Starts a container returning the id of that container.
    ///
    /// TODO: If this fails it may leave the container runtime in an invalid
    /// state.
    pub async fn start_container(
        self: &Arc<Self>,
        container_config: &ContainerConfig,
    ) -> Result<String> {
        let mut container_id = vec![0u8; 16];
        crypto::random::global_rng()
            .generate_bytes(&mut container_id)
            .await;

        let container_id = base_radix::hex_encode(&container_id);

        // TODO: Also lock down permissions on this dir.
        let container_dir = self.run_dir.join(&container_id);
        file::create_dir_all(&container_dir).await?;

        let cgroup_dir = self.cgroup_dir.join(&container_id);
        file::create_dir(&cgroup_dir).await?;

        let mut cgroup = Cgroup::new(cgroup_dir.clone());
        cgroup.set_max_pids(100_000).await?;

        // TODO: Also add rlimit/ulimits for stuff like max number of fds.

        // NOTE: We use 'clone()' instead of 'fork()' to immediately put the sub-process
        // into a new PID namespace ('unshare()' requires an extra fork for
        // that)

        let mut stdin = Mutex::new(None);
        let mut output_streams = vec![];
        let mut file_mapping = FileMapping::default();

        let (mut socket_p, mut socket_c) = SetupSocket::create()?;

        if !container_config.process().terminal() {
            // TODO: Immediately wrap the fds in File objects.
            // TODO: No longer need stdin
            let stdin_read = FileReference::path("/dev/null");
            let (stdout_read, stdout_write) = FileReference::pipe()?;
            let (stderr_read, stderr_write) = FileReference::pipe()?;

            // TODO: Do something with stdin.
            file_mapping
                .add(STDIN, stdin_read)
                .add(STDOUT, stdout_write)
                .add(STDERR, stderr_write);

            output_streams = vec![
                (LogStream::STDOUT, stdout_read.open()?.into()),
                (LogStream::STDERR, stderr_read.open()?.into()),
            ];
        }

        // TODO: Move this logic into the Cgroup struct (it's useful for implementing
        // openat). TODO: Just open with O_PATH.
        let cgroup_file = file::LocalFile::open(&cgroup_dir)?;

        /*
        let parent_pid = parent_container.pid.as_raw();

        let mut parent_pid_file = {
            // NOTE: This opens it with CLOSE_ON_EXEC by default.
            let pidfd = unsafe { libc::syscall(libc::SYS_pidfd_open, parent_pid, 0) };
            if pidfd == -1 {
                return Err(err_msg("Failed to open pidfd"));
            }

            Some(unsafe { std::fs::File::from_raw_fd(pidfd as libc::pid_t) })
        };

        // TODO: Should I unset this later?
        nix::sched::setns(parent_pid_file.as_ref().unwrap().as_raw_fd(), CloneFlags::CLONE_NEWPID)?;
        */

        // NOTE: We must lock this before clone() and until we insert the new container
        // to ensure that waitpid() doesn't return before the container is in
        // the list.
        //
        // This is similar in purpose to the trick of running sigprocmask() to mask
        // SIGCHLD and then unmask it after the job has been added to the list.
        // That trick won't work in our case though as we could be running the
        // waitpid() loop in a separate thread.
        let mut containers = self.containers.lock().await;

        // TODO: implement the waiter strategy and create a new uid_map and gid_map and
        // disallow adding groups.

        // TODO: CLONE_INTO_CGROUP
        // TODO: Can memory (e.g. keys from the parent progress be read after the fork
        // and do we need security against this?).
        //
        // TODO: Use CloneFlags::CLONE_CLEAR_SIGHAND. Currently it is Ok for the child
        // to receive signals though as we send the signals through an channel
        // before processing them. Because the async framework will stop running
        // when cloned, we will never take any actions in response to signals.
        //
        // TODO: Ideally we should use sigprocmask() to temporarily disable signals
        // until the child process sets up signal handlers. It should be noted
        // that by default the init process won't be killed by SIGINT|SIGTERM so
        // if we ask to immediately kill a container after it is started, the
        // container init process may not notice until
        let pid = sys::CloneArgs::new()
            .flags(
                sys::CloneFlags::CLONE_NEWUSER
                    | sys::CloneFlags::CLONE_NEWPID
                    | sys::CloneFlags::CLONE_NEWNS
                    | sys::CloneFlags::CLONE_NEWIPC
                    | sys::CloneFlags::CLONE_INTO_CGROUP
                    | sys::CloneFlags::CLONE_NEWCGROUP,
            )
            .sigchld()
            .cgroup(unsafe { cgroup_file.as_raw_fd() })
            .spawn_process(|| {
                run_child_process(
                    &container_config,
                    container_dir.as_ref(),
                    &mut socket_c,
                    &file_mapping,
                )
            })?;

        let (event_sender, event_receiver) = channel::bounded(1);

        let mut meta = ContainerMetadata::default();
        meta.set_id(container_id.clone());
        meta.set_state(ContainerState::Creating);

        containers.push(Container {
            metadata: meta,
            directory: container_dir.to_owned(),
            cgroup,
            pid,
            // TODO: Revert to always being a non-Option type so that it can be cancelled easily.
            waiter_task: None,
            event_sender,
            stdin: Mutex::new(None),
        });

        // Arc::new(Mutex::new(stdin_write.open()?.into()))

        drop(containers);

        // Drop in the parent process.
        drop(file_mapping);
        drop(socket_c);

        // TODO: If anything below this point fails, should we kill the container?

        // For now just copy the uid/gid maps of the parent.
        // NOTE: Because this contains the user that runs the main cluster_node
        // process, we should never give the user CAP_SETUID in this namespace.
        //
        // TODO: Before writing the gid_map, we should write "/proc/[pid]/setgroups".
        // But this requires that we have already called setgroups for the child to
        // initialize to the starter set of groups.
        // Maybe we can just clone into a new PID namespace and then later create a new
        // user namespace?
        file::copy("/proc/self/uid_map", format!("/proc/{}/uid_map", pid)).await?;
        file::copy("/proc/self/gid_map", format!("/proc/{}/gid_map", pid)).await?;

        socket_p.notify(USER_NS_SETUP_BYTE)?;

        // Receive the TTY

        // TODO: Must receive and use it for our logging, etc.
        // just one logging instance though

        if container_config.process().terminal() {
            let terminal_file = socket_p.recv_fd(TERMINAL_FD_BYTE)?;
            let terminal_file_2 = terminal_file.try_clone()?;

            output_streams = vec![(LogStream::STDOUT, terminal_file.into())];

            stdin = Mutex::new(Some(terminal_file_2.into()));
        }

        // TIOCSWINSZ

        // TODO: Setup console size (default 80 x 24)

        {
            let waiter_task = executor::spawn(self.clone().container_waiter(ContainerWaiter {
                container_id: container_id.clone(),
                container_dir: container_dir.clone(),
                output_streams,
                event_receiver,
            }));

            let mut containers = self.containers.lock().await;

            // TODO: Remove the unwrap. If it is removed, then that means that we probably
            // need to close files, etc.
            let mut container = containers
                .iter_mut()
                .find(|c| c.metadata.id() == container_id)
                .unwrap();

            container.metadata.set_state(ContainerState::Running);
            container.stdin = stdin;
            container.waiter_task = Some(waiter_task);
        }

        socket_p.notify(FINISHED_SETUP_BYTE)?;

        Ok(container_id)
    }

    // TODO: Verify that this doesn't fail if the container has already been stopped
    // as this may be a race condition.
    pub async fn kill_container(&self, container_id: &str, signal: sys::Signal) -> Result<()> {
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
        unsafe { sys::kill(container.pid, signal)? };
        Ok(())
    }

    pub async fn open_log(&self, container_id: &str) -> Result<FileLogReader> {
        let container_dir = self.run_dir.join(container_id);

        // TODO: If the container isn't currently running, the FileLogReader should
        // indicate that to the user (e.g. if no end of stream entries are present in
        // the log file, it should return an end of stream indicator anyway).
        let is_running = {
            let containers = self.containers.lock().await;
            containers
                .iter()
                .find(|c| c.metadata.id() == container_id)
                .map(|c| c.metadata.state() != ContainerState::Stopped)
                .unwrap_or(false)
        };

        if !file::exists(&container_dir).await? {
            return Err(rpc::Status::not_found(format!(
                "No data for container with id: {}",
                container_id
            ))
            .into());
        }

        let log_path = container_dir.join("log");

        // TODO: For this to work we need to ensure that the log file is synchronously
        // created before we return the container id to the person that
        // reuqested it.
        FileLogReader::open(&log_path).await
    }

    pub async fn write_to_stdin(&self, container_id: &str, data: &[u8]) -> Result<()> {
        let containers = self.containers.lock().await;
        let container = containers
            .iter()
            .find(|c| c.metadata.id() == container_id)
            .ok_or_else(|| err_msg("Container not found"))?;

        let mut file = container
            .stdin
            .lock()
            .await
            .take()
            .ok_or_else(|| err_msg("Container has no stdin"))?;

        drop(containers);

        // TODO: Need locking to ensure that we only have one writer of this.

        file.write_all(data).await?;
        file.flush().await?; // async_std internally buffers writes.

        Ok(())
    }

    async fn write_log(file: LocalFile, log_writer: Arc<FileLogWriter>, stream: LogStream) {
        if let Err(e) = log_writer.write_stream(file, stream).await {
            eprintln!("Log writer failed: {:?}", e);
        }
    }

    async fn container_waiter(self: Arc<Self>, input: ContainerWaiter) {
        let res = self.container_waiter_inner(input).await;

        // TODO: Some types of errors should take down the entire server and others
        // should be isolated to invalidating this container.
        if let Err(e) = res {
            eprintln!("Container waiter error: {:?}", e);
        }
    }

    async fn container_waiter_inner(self: Arc<Self>, input: ContainerWaiter) -> Result<()> {
        let log_writer = Arc::new(FileLogWriter::create(&input.container_dir.join("log")).await?);

        let mut log_tasks = vec![];
        for (stream, file) in input.output_streams {
            log_tasks.push(ChildTask::spawn(Self::write_log(
                file,
                log_writer.clone(),
                stream,
            )));
        }

        let status = input.event_receiver.recv().await?;

        for task in log_tasks {
            task.join().await;
        }

        // TODO: Flush the log? (fsync, etc.)

        let container_id = input.container_id.as_str();

        let mut containers = self.containers.lock().await;
        let container = containers
            .iter_mut()
            .find(|c| c.metadata.id() == container_id)
            .unwrap();

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
