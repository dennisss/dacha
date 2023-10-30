#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;
#[macro_use]
extern crate file;

use common::{bytes::Bytes, errors::*};

#[executor_main]
async fn main() -> Result<()> {
    let files = &[
        ("content_decryption_module.h", "https://chromium.googlesource.com/chromium/cdm/+/refs/heads/main/content_decryption_module.h?format=TEXT"),
        ("content_decryption_module_export.h", "https://chromium.googlesource.com/chromium/cdm/+/refs/heads/main/content_decryption_module_export.h?format=TEXT"),
    ];

    let client = http::SimpleClient::new(http::SimpleClientOptions::default());

    for (file_name, url) in files {
        let req = http::RequestBuilder::new()
            .method(http::Method::GET)
            .uri(*url)
            .build()?;

        let res = client
            .request(
                &req.head,
                Bytes::new(),
                &http::ClientRequestContext::default(),
            )
            .await?;

        let data = base_radix::base64_decode(std::str::from_utf8(&res.body)?)?;

        file::write(
            project_path!("third_party/chromium/cdm/repo").join(file_name),
            &data,
        )
        .await?;
    }

    Ok(())
}
