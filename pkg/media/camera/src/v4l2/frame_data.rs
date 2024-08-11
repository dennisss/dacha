use executor::channel;

use crate::frame::*;

pub(super) struct V4L2ImageFrameData {
    pub buf: Option<v4l2::MMAPBuffer>,
    pub returner: channel::Sender<v4l2::MMAPBuffer>,
}

impl Drop for V4L2ImageFrameData {
    fn drop(&mut self) {
        let buf = self.buf.take().unwrap();
        let _ = self.returner.try_send(buf);
    }
}

impl ImageFrameData for V4L2ImageFrameData {
    fn data<'a>(&'a self) -> Option<&'a [u8]> {
        let buf = self.buf.as_ref().unwrap();
        Some(buf.used_memory())
    }

    fn dma_buffer(&self) -> Option<DMABuffer> {
        None
    }
}
