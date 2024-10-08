use common::bytes::Bytes;
use common::errors::*;
use executor_graph::*;
use video::h264::{NALUnitHeader, NALUnitType};

use crate::frame::*;

/// This op caches the init data from the frame in an H264 frame stream and
/// continously returns it in future frames (this is to ensure that any readers
/// that start reading late can still process the stream at the next keyframe).
pub struct H264BufferOp {
    //
}

impl H264BufferOp {
    pub fn new() -> Self {
        Self {}
    }

    async fn execute_impl(&self, mut input: InputStream, mut output: OutputStream) -> Result<()> {
        let mut pps = None;
        let mut sps = None;

        while let Some(input_any) = input.read().await? {
            let input_frame = input_any.downcast_ref::<ImageFrame>().unwrap();

            // TODO: Only do for H264 data.
            Self::find_h264_stream_init_data(input_frame.data.data().unwrap(), &mut pps, &mut sps)?;

            let pps = pps
                .clone()
                .ok_or_else(|| err_msg("Camera stream missing PPS"))?;
            let sps = sps
                .clone()
                .ok_or_else(|| err_msg("Camera stream missing SPS"))?;

            let mut output_frame = input_frame.clone();
            output_frame.init_data = vec![pps, sps];

            output.write(Box::new(output_frame)).await?;
        }

        output.close().await;

        Ok(())
    }

    fn find_h264_stream_init_data(
        data: &[u8],
        pps: &mut Option<Bytes>,
        sps: &mut Option<Bytes>,
    ) -> Result<()> {
        if pps.is_some() && sps.is_some() {
            return Ok(());
        }

        let mut iter = video::h264::H264BitStreamIterator::new(data);

        while let Some(nalu) = iter.peek() {
            let (header, rest) = NALUnitHeader::parse(nalu.data())?;
            match header.nal_unit_type {
                NALUnitType::PPS => {
                    // TODO: We'd want to tack the whole NALU
                    *pps = Some(nalu.raw().into());
                }
                NALUnitType::SPS => {
                    *sps = Some(nalu.raw().into());
                }
                _ => {}
            }

            // TODO: Make this simpler.
            nalu.advance();
        }

        Ok(())
    }
}

#[async_trait]
impl Operation for H264BufferOp {
    fn signature(&self) -> OperationSignature {
        OperationSignature {
            name: "H264Buffer".to_string(),
            num_inputs: 1,
            num_outputs: 1,
        }
    }

    async fn execute(
        &self,
        mut inputs: Vec<InputStream>,
        mut outputs: Vec<OutputStream>,
    ) -> Result<()> {
        self.execute_impl(inputs.pop().unwrap(), outputs.pop().unwrap())
            .await
    }
}
