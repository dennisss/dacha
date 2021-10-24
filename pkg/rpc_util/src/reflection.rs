use common::errors::*;
use grpc_proto::reflection::*;

pub trait AddReflection {
    /// Adds reflection capabilities to an RPC server.
    ///
    /// This allows a client to introspect the provided services and their type
    /// descriptors.
    ///
    /// NOTE: Must be called AFTER all services have been added to the server.
    fn add_reflection(&mut self) -> Result<()>;
}

impl AddReflection for rpc::Http2Server {
    fn add_reflection(&mut self) -> Result<()> {
        let reflection_service = ServerReflectionImpl::new(self);
        self.add_service(reflection_service.into_service())?;
        Ok(())
    }
}

struct ServerReflectionImpl {
    services: Vec<(&'static str, &'static protobuf::StaticFileDescriptor)>,
}

impl ServerReflectionImpl {
    fn new(server: &rpc::Http2Server) -> Self {
        let mut services = vec![];
        for s in server.services() {
            services.push((s.service_name(), s.file_descriptor()));
        }

        Self { services }
    }

    async fn call<'a>(
        &self,
        mut request: rpc::ServerStreamRequest<ServerReflectionRequest>,
        response: &mut rpc::ServerStreamResponse<'a, ServerReflectionResponse>,
    ) -> Result<()> {
        while let Some(req) = request.recv().await? {
            let mut res = ServerReflectionResponse::default();

            match req.message_request_case() {
                ServerReflectionRequestMessageRequestCase::FileContainingSymbol(name) => {
                    let (_, desc) =
                        self.services
                            .iter()
                            .find(|(n, _)| n == name)
                            .ok_or_else(|| {
                                Error::from(rpc::Status::invalid_argument("Unknown symbol"))
                            })?;

                    let res = res.file_descriptor_response_mut();
                    Self::append_descriptor(*desc, res);
                }
                ServerReflectionRequestMessageRequestCase::ListServices(_) => {
                    let res = res.list_services_response_mut();
                    for (service_name, _) in &self.services {
                        let mut r = ServiceResponse::default();
                        r.set_name(*service_name);
                        res.add_service(r);
                    }
                }
                _ => {
                    return Err(rpc::Status::invalid_argument("Request type notsupported").into());
                }
            }

            response.send(res).await?;
        }

        Ok(())
    }

    fn append_descriptor(desc: &protobuf::StaticFileDescriptor, out: &mut FileDescriptorResponse) {
        out.add_file_descriptor_proto(desc.proto.into());
        for dep in desc.dependencies {
            Self::append_descriptor(*dep, out);
        }
    }
}

#[async_trait]
impl ServerReflectionService for ServerReflectionImpl {
    async fn ServerReflectionInfo(
        &self,
        request: rpc::ServerStreamRequest<ServerReflectionRequest>,
        response: &mut rpc::ServerStreamResponse<ServerReflectionResponse>,
    ) -> Result<()> {
        self.call(request, response).await
    }
}
