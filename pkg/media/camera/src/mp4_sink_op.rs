use common::errors::*;

use crate::frame::*;
use crate::graph::*;

/*
let mp4_builder = MP4Builder::new(
    width as u32,
    height as u32,
    FRAME_RATE as u32,
    MP4BuilderOptions::default(),
)?;

/// Input: Stream of H264 video chunks
/// Output: MP4 file written to disk.
async fn encoder_outfeed_task(
    encoder: Arc<H264Encoder>,
    mut mp4_builder: MP4Builder,
    inputs: channel::Receiver<()>,
) -> Result<()> {
    println!("Start outfeed");

    let mut i = 0;

    while let Ok(()) = inputs.recv().await {
        // TODO: Make sure that this eventually gets cancelled.
        let capture_buffer = encoder.dequeue_data().await?;

        // TODO: Propagate the frame timestamps.
        mp4_builder.append(capture_buffer.used_memory(), None, false)?;



        {
            let request = encoder.dequeue_frame().await?;
            drop(request);
        }
    }

    mp4_builder.append(&[], None, true)?;

    let mut out = vec![];
    while let Some(event) = mp4_builder.consume() {
        out.extend_from_slice(&event.data);
    }

    file::write("image.mp4", out).await?;

    println!("Done outfeed");

    Ok(())
}
*/

const NUM_FRAMES: usize = 5 * 30;

pub struct MP4SinkOp {
    //
}

impl MP4SinkOp {
    pub fn new() -> Self {
        Self {}
    }

    async fn execute_impl(&self, mut input: InputStream) -> Result<()> {
        for _ in 0..NUM_FRAMES {
            let input_any = match input.read().await {
                Some(v) => v,
                None => break,
            };

            let input_frame = input_any.downcast_ref::<ImageFrame>().unwrap();
        }

        Ok(())
    }
}

#[async_trait]
impl Operation for MP4SinkOp {
    fn signature(&self) -> OperationSignature {
        OperationSignature {
            name: "MP4Sink".to_string(),
            num_inputs: 1,
            num_outputs: 0,
        }
    }

    async fn execute(
        &self,
        mut inputs: Vec<InputStream>,
        mut outputs: Vec<OutputStream>,
    ) -> Result<()> {
        self.execute_impl(inputs.pop().unwrap()).await
    }
}
