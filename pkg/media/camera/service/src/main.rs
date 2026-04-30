
/*

We are capturing V4L2_PIX_FMT_H264
- "H264 video elementary stream with start codes."

- H264 in RTP: https://www.rfc-editor.org/rfc/rfc6184

    - Byte stream is the "elementary" format (https://wenchy.github.io/blogs/2015-12-11-H.264-stream-structure.html)
    - Wrapper around NAL

*/

// TODO: Fix the file API to remap EACCESS

// TODO: ioctl needs to retry EINTR

// TODO: Disable any dynamic feature like auto-exposure or AWB

// TODO: Set FrameDurationLimits : [i64; 2] where each value should be the frame
// time in microseconds. ^ These should be passed to Camera::start() : Or check
// to see if

// TODO: Ensure that CameraConfiguration::transform is empty (identity)

/*
Basically goal is to:
- Capture two frames
- Invocation one:
    - Apply a gaussian blur to them (OpenGL kernel?)
        - Also used to copy the frame
- Invocation two:
    - Diff the two frames
    - Sum up the number of pixel values that have changed.
-


General motion detection pipeline:
- Maintain a last_frame which is updated every 10 seconds with a 5 second old frame
- Every new frame is compared about last_frame
- Require 5 consecutive frames to
- Disable motion tracking while home (home is on the wifi network, except bedtime tracking)
    - External trigger if things like door sensors or PIR are triggered
- Will also need auto-switching of IR
- Validation
    - At least 10 seconds of motion per day
    - No more than 1 hour of motion per day
    - Verify frame is similar to frame from a few days ago


*/

#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use std::{collections::HashMap, sync::Arc};

use common::{errors::*, io::Readable};
use http::ServerHandler;
use media_camera::camera_manager::{CameraEntry, CameraManager};
use media_camera_proto::media::camera::*;
use parsing::ascii::AsciiString;
use rpc_util::NamedPortArg;
use web::WebPageHandler;
use executor_multitask::RootResource;
use cluster_client::{ClusterMetaClient, ClusterServer};
use http_util::extract_path_params;

const SERVICE_ACL_PROTO: &'static str = r#"
    rules: [
        {
            path: "/"
            is_directory: false
            principals: ["authenticated"]
        },
        {
            path: "/camera"
            is_directory: true
            principals: ["group:cluster-owners"]
        },
        {
            path: "/rpc/media.camera.CameraInterface"
            is_directory: true
            principals: ["group:cluster-owners"]
        }
    ]
"#;


#[derive(Args)]
struct Args {
    /// Port on which to start the web server.
    port: NamedPortArg,
}

struct HttpHandler {
    camera_manager: CameraManager,
}

impl HttpHandler {
    async fn handle_request_impl<'a>(
        &self,
        mut request: http::Request,
        context: http::ServerRequestContext<'a>,
    ) -> Result<http::Response> {

        let path = request.head.uri.path.as_str();

        let mut params = match extract_path_params(path, "/camera/:camera_id") {
            Some(v) => v,
            None => return Ok(http_util::not_found())
        };

        let camera_id = http_util::decode_uri_component(&params.remove("camera_id").unwrap());

        let mut entries = self.camera_manager.list().await?;

        let entry = match entries.remove(&camera_id) {
            Some(v) => v,
            None => return Ok(http_util::not_found())
        };

        let camera = self.camera_manager.open(entry).await?;
        media_camera::camera_stream::respond_with_camera_stream(camera).await
    }
}

#[async_trait]
impl http::ServerHandler for HttpHandler {
    async fn handle_request<'a>(
        &self,
        request: http::Request,
        context: http::ServerRequestContext<'a>,
    ) -> http::Response {
        match self.handle_request_impl(request, context).await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("{}", e);
                http_util::internal_server_error()
            }
        }
    }
}

pub struct ServiceImpl {
    camera_manager: CameraManager,
}

impl ServiceImpl {
    async fn list_cameras_impl(&self) -> Result<ListCamerasResponse> {
        let mut res = ListCamerasResponse::default();

        let entries = self.camera_manager.list().await?;

        for (id, entry) in entries {
            let proto = res.new_entries();
            let name = entry.name().await?;

            proto.set_id(id);
            proto.set_name(name);
        }

        Ok(res)
    }

    async fn get_properties_impl(
        &self,
        request: &GetPropertiesRequest,
    ) -> Result<GetPropertiesResponse> {
        let mut entries = self.camera_manager.list().await?;

        let entry = entries
            .remove(request.camera_id())
            .ok_or_else(|| rpc::Status::not_found("No camera with given id"))?;

        let camera = self.camera_manager.open(entry).await?;

        let mut out = GetPropertiesResponse::default();
        out.set_properties(camera.properties().await?);
        out.set_format(camera.format_proto().await?);

        Ok(out)
    }

    async fn set_properties_impl(
        &self,
        request: &SetPropertiesRequest,
    ) -> Result<()> {
        let mut entries = self.camera_manager.list().await?;

        let entry = entries
            .remove(request.camera_id())
            .ok_or_else(|| rpc::Status::not_found("No camera with given id"))?;

        let camera = self.camera_manager.open(entry).await?;

        camera.set_properties(request.state()).await?;

        Ok(())
    }

}

#[async_trait]
impl CameraInterfaceService for ServiceImpl {
    async fn ListCameras(
        &self,
        request: rpc::ServerRequest<ListCamerasRequest>,
        response: &mut rpc::ServerResponse<ListCamerasResponse>,
    ) -> Result<()> {
        response.value = self.list_cameras_impl().await?;
        Ok(())
    }

    async fn GetProperties(
        &self,
        request: rpc::ServerRequest<GetPropertiesRequest>,
        response: &mut rpc::ServerResponse<GetPropertiesResponse>,
    ) -> Result<()> {
        response.value = self.get_properties_impl(&request.value).await?;
        Ok(())
    }

    async fn SetProperties(
        &self,
        request: rpc::ServerRequest<SetPropertiesRequest>,
        response: &mut rpc::ServerResponse<SetPropertiesResponse>,
    ) -> Result<()> {
        self.set_properties_impl(&request.value).await?;
        Ok(())
    }
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let root_resource = executor_multitask::RootResource::new();

    let client = ClusterMetaClient::create_from_environment().await?;
    root_resource.register_dependency(client.clone()).await;

    let camera_manager = CameraManager::create()?;

    let service_impl = ServiceImpl {
        camera_manager: camera_manager.clone(),
    };


    let mut acl = cluster_proto::cluster::ServiceACLProto::default();
    protobuf::text::parse_text_proto(SERVICE_ACL_PROTO, &mut acl)?;

    let mut server = ClusterServer::new(args.port.value(), acl, client.clone())?;
    server.add_service(service_impl.into_service())?;

    let web_handler = Arc::new(WebPageHandler::create(web::WebPageOptions {
        title: "Media Camera".into(),
        script_path: "built/pkg/media/camera/app.js".into(),
        vars: None,
    }).await?);
    server.add_request_handler("/", false, web_handler.clone())?;

    let camera_handler = Arc::new(HttpHandler {
        camera_manager,
    });
    server.add_request_handler("/camera", true, camera_handler.clone())?;

    root_resource.register_dependency(server.start()?).await;

    println!("Ready!");

    root_resource.wait().await
}
