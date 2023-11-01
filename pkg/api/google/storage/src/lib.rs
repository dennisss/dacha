/*
TODO: Do big uploads with parallel composite uuploads:
- https://cloud.google.com/storage/docs/parallel-composite-uploads

Resumable upload:
- https://cloud.google.com/storage/docs/performing-resumable-uploads

Streaming downloads:
- https://cloud.google.com/storage/docs/streaming-downloads
- (Need to do our own checksum checks though)

*/

use google_discovery_generated::storage_v1;

pub struct Client {
    //
}
