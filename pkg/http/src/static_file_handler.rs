use common::errors::*;
use common::io::Readable;
use common::async_std::path::{PathBuf, Path};
use common::async_std::fs::File;

use crate::server::RequestHandler;
use crate::request::Request;
use crate::response::{Response, ResponseBuilder};
use crate::status_code;
use crate::body::*;


/// HTTP request handler which serves static files from the local file system.
pub struct StaticFileHandler {
    // mount_path: UriPath,
    base_path: PathBuf
    
    // Need to be able to detect content types of files (either from extensions or binary)
    // Need to be able to know if a content type is compressable (or if it is already compressed)
    
    // TODO: Need to support Last-Modified and ETag stuff (will be difficult if we need to store the entire thing in memory)
}

impl StaticFileHandler {
    pub fn new<P: AsRef<Path>>(base_path: P) -> Self {
        Self { base_path: base_path.as_ref().into() }
    }
}

#[async_trait]
impl RequestHandler for StaticFileHandler {
    async fn handle_request(&self, request: Request) -> Response {
        let mut file_path = self.base_path.clone();
        
        let mut segments = request.head.uri.path.as_ref().split('/');
        
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

        let metadata = match file_path.metadata().await {
            Ok(m) => m,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    return ResponseBuilder::new()
                        .status(status_code::NOT_FOUND)
                        .build().unwrap();
                }

                return ResponseBuilder::new()
                    .status(status_code::INTERNAL_SERVER_ERROR)
                    .build().unwrap();
            }
        };
        
        // Only regular files are supported.
        if !metadata.is_file() {
            return ResponseBuilder::new()
                        .status(status_code::BAD_REQUEST)
                        .build().unwrap();
        }

        let file = match File::open(&file_path).await {
            Ok(f) => f,
            Err(_) => {
                return ResponseBuilder::new()
                    .status(status_code::INTERNAL_SERVER_ERROR)
                    .build().unwrap();
            }
        };

        let body = StaticFileBody {
            file, length: metadata.len() as usize
        };

        ResponseBuilder::new()
            .status(status_code::OK)
            .body(Box::new(body))
            .build()
            .unwrap()
    }
}


pub struct StaticFileBody {
    file: File,
    length: usize,
}

impl StaticFileBody {

}

#[async_trait]
impl Body for StaticFileBody {
    fn len(&self) -> Option<usize> {
        Some(self.length)
    }

    async fn trailers(&mut self) -> Result<Option<crate::header::Headers>> {
        Ok(None)
    }
}

#[async_trait]
impl Readable for StaticFileBody {
    // TODO: If the file changed since reading it, return an error (if possible?)
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        // TODO: Ensure that we are buffering based on file system chunk sizes.
        Ok(self.file.read(buf).await?)
    }
}


