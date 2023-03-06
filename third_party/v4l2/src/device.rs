use std::{collections::HashSet, sync::Arc};

use base_error::*;
use executor::child_task::ChildTask;
use executor::sync::Mutex;
use executor::Condvar;
use executor::ExecutorPollingContext;
use file::{LocalFile, LocalFileOpenOptions, LocalPath};
use sys::EpollEvents;

use crate::bindings::*;
use crate::io::*;
use crate::stream::*;

pub struct Device {
    handle: Arc<DeviceHandle>,
    streams: HashSet<v4l2_buf_type>,
}

pub(crate) struct DeviceHandle {
    /// Task used to poll for
    polling_task: ChildTask,

    pub shared: Arc<DeviceShared>,
}

pub(crate) struct DeviceShared {
    /// File for this device.
    ///
    /// Thread safety is not well defined for all drivers so we require that all
    /// ioctl commands are performed under a file lock.
    ///
    /// See also https://stackoverflow.com/questions/10217779/how-thread-safe-is-v4l2#:~:text=ioctl()%20is%20not%20one,once%20it%20reaches%20ioctl()
    pub file: Condvar<LocalFile>,
}

impl Device {
    pub fn open<P: AsRef<LocalPath>>(path: P) -> Result<Self> {
        let file = file::LocalFile::open_with_options(
            path,
            &LocalFileOpenOptions::new()
                .read(true)
                .write(true)
                .non_blocking(true),
        )?;

        let shared = Arc::new(DeviceShared {
            file: Condvar::new(file),
        });

        Ok(Self {
            handle: Arc::new(DeviceHandle {
                polling_task: ChildTask::spawn(Self::polling_thread(shared.clone())),
                shared,
            }),
            streams: HashSet::new(),
        })
    }

    pub async fn print_capabiliites(&self) -> Result<()> {
        let file = self.handle.shared.file.lock().await;

        let mut caps = v4l2_capability::default();
        unsafe { vidioc_querycap(file.as_raw_fd(), &mut caps) }?;

        println!("Driver: {}", read_null_terminated_string(&caps.driver)?);
        println!("Card: {}", read_null_terminated_string(&caps.card)?);
        println!("Bus Info: {}", read_null_terminated_string(&caps.bus_info)?);

        Ok(())
    }

    pub fn new_stream(&mut self, typ: v4l2_buf_type) -> Result<UnconfiguredStream> {
        if !self.streams.insert(typ) {
            return Err(format_err!(
                "Already configuring a stream with buffer type {:?}",
                typ
            ));
        }

        // TODO: Validate that we were given a multi-plane type as the rest of the
        // stream/buffer code assumes this.

        Ok(UnconfiguredStream {
            device: self.handle.clone(),
            typ,
        })
    }

    async fn polling_thread(shared: Arc<DeviceShared>) {
        if let Err(e) = Self::polling_thread_inner(&shared).await {
            eprintln!("V4L2 Polling Error: {}", e);

            // We assume that the users call ioctl, linux will return errors (so we don't
            // need to store this error).
            shared.file.lock().await.notify_all();
        }
    }

    async fn polling_thread_inner(shared: &DeviceShared) -> Result<()> {
        let mut ctx = {
            let file = shared.file.lock().await;
            unsafe {
                ExecutorPollingContext::create_with_raw_fd(file.as_raw_fd(), EpollEvents::EPOLLIN)
                    .await
            }?
        };

        loop {
            let mut events = ctx.wait().await?;

            if events.contains(EpollEvents::EPOLLIN) {
                events = events.remove(EpollEvents::EPOLLIN);

                shared.file.lock().await.notify_all();
            }

            if events != EpollEvents::empty() {
                // We will get an EPOLLERR until all the streams are turned up.
                // TODO: Get back to a state where we can return these errors.s
                // (Also ensure all Rust side waiters are aware when this happens).
                eprintln!("Unknown poll events received: {:?}", events);
            }
        }
    }

    // pub
}

// TODO: Deduplicate this everywhere.
fn read_null_terminated_string(data: &[u8]) -> Result<String> {
    for i in 0..data.len() {
        if data[i] == 0x00 {
            return Ok(std::str::from_utf8(&data[0..i])?.to_string());
        }
    }

    Err(err_msg("Missing null terminator"))
}
