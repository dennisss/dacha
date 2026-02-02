use std::collections::HashMap;

use common::errors::*;
use common::hash::FastHasherBuilder;
use common::io::Readable;
use file::{LocalFile, LocalPath, LocalPathBuf};

use crate::body::*;
use crate::header::*;
use crate::headers::range::parse_range_header;
use crate::request::Request;
use crate::response::{Response, ResponseBuilder};
use crate::server_handler::{ServerHandler, ServerRequestContext};
use crate::status_code;

#[derive(Default)]
pub struct StaticFileHandlerOptions {
    // TODO: In the rpc code, we call this the 'base_path', so maybe standardize this naming better.
    pub mount_path: String,

    /// If true, infer and return a Content-Type header based on the file
    /// extension of the requested path. By default, this is false and the
    /// Content-Type is always application/octet-stream.
    pub trust_file_extension: bool,
}

/// HTTP request handler which serves static files from the local file system.
pub struct StaticFileHandler {
    base_path: LocalPathBuf, /* Need to be able to detect content types of files (either from
                              * extensions or binary) Need to be able to know
                              * if a content type is compressable (or if it is already
                              * compressed) */

    /* TODO: Need to support Last-Modified and ETag stuff (will be difficult
     * if we need to store the entire thing in memory) */
    options: StaticFileHandlerOptions,

    extension_types: HashMap<&'static str, &'static str, FastHasherBuilder>,
}

impl StaticFileHandler {
    pub fn new<P: AsRef<LocalPath>>(base_path: P) -> Self {
        Self::new_with_options(base_path, StaticFileHandlerOptions::default())
    }

    pub fn new_with_options<P: AsRef<LocalPath>>(
        base_path: P,
        options: StaticFileHandlerOptions,
    ) -> Self {
        let mut extension_types = HashMap::default();
        for typ in mime_types::MEDIA_TYPES_LIST {
            for ext in typ.extensions {
                // NOTE: We assume that 'ext' is in lowercase.
                extension_types.insert(*ext, typ.types[0]);
            }
        }

        Self {
            base_path: base_path.as_ref().to_owned(),
            options,
            extension_types,
        }
    }
}

#[async_trait]
impl ServerHandler for StaticFileHandler {
    async fn handle_request<'a>(&self, request: Request, _: ServerRequestContext<'a>) -> Response {
        let mut file_path = self.base_path.clone();

        let path = request.head.uri.path.as_str()
            .strip_prefix(&self.options.mount_path).unwrap_or("");

        let mut segments = path.split('/');

        // Switch the initial empty segment before the first '/'
        segments.next();

        // TODO: Ensure no .. or .
        // TODO: Validate that the Uri contains nothing but a path.
        // TODO: Decode URI components.

        // TODO: First steps is

        for segment in segments {
            // Interpet each path segment as UTF-8.
            let segment_str = {
                segment

                // if let Ok(s) = segment.to_utf8_str() {
                //     s
                // } else {
                //     return ResponseBuilder::new()
                //         .status(status_code::BAD_REQUEST)
                //         .build().unwrap();
                // }
            };

            file_path.push(segment_str);
        }

        // TODO: Validate that the path is in the 'base_path'. Though when running in a ClusterServer, the path normalization done before this will always guarantee this.

        let metadata = match file::metadata(&file_path).await {
            Ok(m) => m,
            Err(e) => {
                if file::because_file_doesnt_exist(&e) {
                    return ResponseBuilder::new()
                        .status(status_code::NOT_FOUND)
                        .build()
                        .unwrap();
                }

                return ResponseBuilder::new()
                    .status(status_code::INTERNAL_SERVER_ERROR)
                    .build()
                    .unwrap();
            }
        };

        // Only regular files are supported.
        if !metadata.is_file() {
            return ResponseBuilder::new()
                .status(status_code::BAD_REQUEST)
                .build()
                .unwrap();
        }

        let mut file = match LocalFile::open(&file_path) {
            Ok(f) => f,
            Err(_) => {
                // TODO: Log an error here.
                return ResponseBuilder::new()
                    .status(status_code::INTERNAL_SERVER_ERROR)
                    .build()
                    .unwrap();
            }
        };

        let mut response = ResponseBuilder::new()
            .status(status_code::OK)
            .header(ACCEPT_RANGES, "bytes");

        if self.options.trust_file_extension {
            // TODO: Lowercase the file extension.

            // TODO: Generalize this. If a client is expected to immediately use a result,
            // we want to specify this, else, we want to allow downloading while preserving
            // the encoding.
            if file_path.as_str().ends_with(".zz") {
                response = response.header(CONTENT_ENCODING, "deflate");
            }

            if let Some(ext) = file_path.extension() {
                if let Some(typ) = self.extension_types.get(ext) {
                    response = response.header(CONTENT_TYPE, *typ);
                }
            }
        }

        let range_header = match parse_range_header(&request.head.headers, metadata.len() as usize)
        {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Invalid Range header: {}", e);
                return ResponseBuilder::new()
                    .status(status_code::BAD_REQUEST)
                    .build()
                    .unwrap();
            }
        };

        let mut range = (0, metadata.len() as usize);

        if let Some((s, e)) = range_header.clone() {
            response = response.status(status_code::PARTIAL_CONTENT).header(
                CONTENT_RANGE,
                format!("bytes {}-{}/{}", s, e, metadata.len()),
            );
            range = (s, e + 1);
        }

        file.seek(range.0 as u64);

        let body = StaticFileBody { file, range };

        response = response.body(Box::new(body));

        response.build().unwrap()
    }
}

pub struct StaticFileBody {
    // NOTE: The file should already be seeked to the start of the range when the StaticFileBody
    // instance is created.
    file: LocalFile,
    range: (usize, usize),
}

impl StaticFileBody {
    pub async fn open(path: &LocalPath) -> Result<Self> {
        let file = LocalFile::open(path)?;
        let length = file.metadata().await?.len() as usize;

        Ok(Self {
            file,
            range: (0, length),
        })
    }
}

#[async_trait]
impl Body for StaticFileBody {
    fn len(&self) -> Option<usize> {
        Some(self.range.1 - self.range.0)
    }

    async fn trailers(&mut self) -> Result<Option<crate::header::Headers>> {
        Ok(None)
    }
}

#[async_trait]
impl Readable for StaticFileBody {
    // TODO: If the file changed since reading it, return an error (if possible?)
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        // TODO: Keep as a u64
        let pos = self.file.current_position() as usize;

        let n = core::cmp::min(buf.len(), self.range.1 - pos);

        // TODO: Ensure that we are buffering based on file system chunk sizes.
        Ok(self.file.read(&mut buf[0..n]).await?)
    }
}
