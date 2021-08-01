use common::errors::*;
use common::io::*;

use crate::body::Body;

// NOTE: Content in the HTTP headers is ISO-8859-1 so may contain characters
// outside the range of ASCII. type HttpStr = Vec<u8>;

// // Body must be either Read or a buffer
// // Regardless must be ov

// TODO: Need validation of duplicate headers.

const BODY_BUFFER_SIZE: usize = 4096;

// TODO: Move this out of the spec as it is the only async thing here.
// Probably move under Body
//
// TODO: The big issue with this is that it may miss any trailers in the body.
// WE need to coordinate this with the headers that are set for Content-Length
// and Transfer-Encoding.
pub async fn write_body(body: &mut dyn Body, writer: &mut dyn Writeable) -> Result<()> {
    // TODO: If we sent a Content-Length, make sure that we are consistent.
    let mut buf = [0u8; BODY_BUFFER_SIZE];
    loop {
        let n = body.read(&mut buf).await?;
        if n == 0 {
            break;
        }

        writer.write_all(&buf[0..n]).await?;
    }

    Ok(())
}
