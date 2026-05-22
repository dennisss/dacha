use std::sync::Arc;
use std::time::{Instant, Duration};

use common::bytes::Bytes;
use common::errors::*;
use executor::sync::AsyncMutex;
use executor::{lock, lock_async};
use executor::channel;
use executor_multitask::{impl_resource_passthrough, ServiceResource, ServiceResourceGroup, BroadcastChannel};
use mocap_proto::mocap::*;
use protobuf_json::*;
use file::project_path;

use crate::timestamp::*;


pub struct DummyMocapCamera {
    frame_rate: u32,
    status: AsyncMutex<MocapCameraStatus>,
    blobs: ReadBlobsResponse,
}

impl DummyMocapCamera {

    pub async fn create(frame_rate: u32) -> Result<Self> {

        let data = file::read_to_string(project_path!("pkg/vision/mocap/camera/js/dummy_status.json")).await?;
        let status = MocapCameraStatus::parse_json(&data, &ParserOptions::default())?;

        let mut blobs = ReadBlobsResponse::default();
        protobuf::text::parse_text_proto(r#"
            cameras: [
                {
                    results {
                        blobs: [
                            {
                                x: 100
                                y: 100
                                radius: 20
                            },
                            {
                                x: 800
                                y: 400
                                radius: 100
                            }
                        ]
                    }                
                }
            ]    

        "#, &mut blobs)?;

        Ok(Self {
            frame_rate,
            status: AsyncMutex::new(status),
            blobs
        })
    }
}


#[async_trait]
impl MocapCameraService for DummyMocapCamera {

    async fn Status(
        &self,
        request: rpc::ServerRequest<StatusRequest>,
        response: &mut rpc::ServerResponse<MocapCameraStatus>
    ) -> Result<()> {
        lock!(status <= self.status.lock().await?, {
            response.value = status.clone();
        });
        Ok(())
    }

    async fn Configure(
        &self,
        request: rpc::ServerRequest<ConfigureRequest>,
        response: &mut rpc::ServerResponse<ConfigureResponse>
    ) -> Result<()> {
        lock!(status <= self.status.lock().await?, {
            status.set_config(request.value.clone());
        });
        Ok(())
    }

    async fn ReadBlobs(
        &self,
        request: rpc::ServerRequest<ReadBlobsRequest>,
        response: &mut rpc::ServerStreamResponse<ReadBlobsResponse>
    ) -> Result<()> {
        response.send_head().await?;

        let interval = Duration::from_secs_f32(1.0 / (self.frame_rate as f32));

        let mut next_frame_time = {
            let start_time: Duration = sys::ClockId::MONOTONIC.get_time()?.into();
            rounded_frame_timestamp(start_time + 2 * interval, self.frame_rate)
        };

        loop {
            let now: Duration = sys::ClockId::MONOTONIC.get_time()?.into();
            if now >= next_frame_time {
                let mut res = self.blobs.clone();
                res.set_frame_timestamp(next_frame_time.as_nanos() as u64);
                response.send(res).await?;

                next_frame_time = rounded_frame_timestamp(next_frame_time + interval, self.frame_rate);
                if now >= next_frame_time {
                    return Err(err_msg("Responding too slowly"));
                }
            }

            executor::sleep(
                (next_frame_time - now).max(Duration::from_micros(500)) 
            ).await?;
        }

        Ok(())
    }

    async fn ReadFrames(
        &self,
        request: rpc::ServerRequest<ReadFramesRequest>,
        response: &mut rpc::ServerStreamResponse<ReadFramesResponse>
    ) -> Result<()> {
        Ok(())
    }


    async fn FlashMCU(
        &self,
        request: rpc::ServerRequest<FlashMCURequest>,
        response: &mut rpc::ServerResponse<FlashMCUResponse>
    ) -> Result<()> {
        Ok(())
    }
}