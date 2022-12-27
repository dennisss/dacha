extern crate alloc;
extern crate core;

extern crate rpc;
#[macro_use]
extern crate common;
extern crate protobuf;
#[macro_use]
extern crate macros;

pub mod proto;

use common::errors::*;
use common::io::Writeable;
use executor::sync::Mutex;
use file::{LocalFile, LocalFileOpenOptions};
use proto::adder::*;

pub struct AdderImpl {
    log_file: Option<Mutex<LocalFile>>,
}

impl AdderImpl {
    pub async fn create(request_log: Option<&str>) -> Result<Self> {
        let log_file = {
            if let Some(path) = request_log {
                Some(Mutex::new(LocalFile::open_with_options(
                    &path,
                    LocalFileOpenOptions::new().append(true).create(true),
                )?))
            } else {
                None
            }
        };

        Ok(Self { log_file })
    }
}

#[async_trait]
impl AdderService for AdderImpl {
    async fn Add(
        &self,
        request: rpc::ServerRequest<AddRequest>,
        response: &mut rpc::ServerResponse<AddResponse>,
    ) -> Result<()> {
        println!("{:?}", request.value);
        response.set_z(request.x() + request.y());
        Ok(())
    }

    async fn AddStreaming(
        &self,
        mut request: rpc::ServerStreamRequest<AddRequest>,
        response: &mut rpc::ServerStreamResponse<AddResponse>,
    ) -> Result<()> {
        while let Some(req) = request.recv().await? {
            println!("{:?}", req);
            let z = req.x() + req.y();

            if let Some(mut file) = self.log_file.as_ref() {
                let mut file = file.lock().await;

                file.write_all(format!("{} + {} = {}\n", req.x(), req.y(), z).as_bytes())
                    .await?;
                file.flush().await?;
            }

            let mut res = AddResponse::default();
            res.set_z(z);
            response.send(res).await?;
        }

        Ok(())
    }
}
