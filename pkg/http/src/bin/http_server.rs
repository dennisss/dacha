#![feature(core_intrinsics, trait_alias)]

#[macro_use]
extern crate common;
extern crate http;
extern crate parsing;
#[macro_use]
extern crate file;

use common::errors::*;
use http::header::*;
use http::status_code::*;

// TODO: Pipelining?

struct Service {}

#[async_trait]
impl http::ServerHandler for Service {
    async fn handle_request<'a>(
        &self,
        req: http::Request,
        ctx: http::ServerRequestContext<'a>,
    ) -> http::Response {
        println!("GOT: {:?}", req);

        if let Some(tls) = &ctx.connection_context.tls {
            if let Some(cert) = &tls.certificate {}
        }

        if let Some(tls) = &ctx.connection_context.tls {
            if let Some(cert) = &tls.certificate {
                println!("SAN: {:?}", cert.subject_alt_name().unwrap());
            } else {
                println!("No certificate");
            }
        }

        let mut data = vec![];
        data.extend_from_slice(b"hello");
        // req.body.read_to_end(&mut data).await;

        // println!("READ: {:?}", data);

        http::ResponseBuilder::new()
            .status(OK)
            .header(CONTENT_TYPE, "text/plain")
            .body(http::BodyFromData(data))
            .build()
            .unwrap()
    }
}

async fn run_server() -> Result<()> {
    // let handler =
    // http::static_file_handler::StaticFileHandler::new(file::project_dir());
    // let handler = http::HttpFn(handle_request);

    let handler = Service {};

    /*
    secp256 is still very slow :(
    */

    let certificate_file = file::read(project_path!("testdata/certificates/server.crt"))
        .await?
        .into();
    let private_key_file = file::read(project_path!("testdata/certificates/server.key"))
        .await?
        .into();

    let mut options = http::ServerOptions::default();

    let mut tls_options =
        crypto::tls::ServerOptions::recommended(certificate_file, private_key_file)?;
    tls_options.certificate_request = Some(crypto::tls::CertificateRequestOptions {
        root_certificate_registry: crypto::tls::CertificateRegistrySource::PublicRoots,
        trust_remote_certificate: true,
    });

    options.tls = Some(tls_options);

    let server = http::Server::new(handler, options);
    server.run(8000).await
}

fn main() -> Result<()> {
    executor::run(run_server())?
}
