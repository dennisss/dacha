use crate::db::version::VersionSet;
use crate::db::Snapshot;

use super::paths::FilePaths;
use super::version_edit::NewFileEntry;

/*
How to snapshot a WAL-less EmbeddedDB:
- Acquire a snapshot which is:
    - serialized VersionSet (can ignore any log stuff though)
    - Every table.

Snapshot format is:
- 4-byte protobuf length
- Followed by a SnapshotManifest
- Followed by concatenated file blobs.

- Big manifest at the start.

Will be serialized as a tar file?

Serializing in cunks may be annoying?

Currently only supported in disable_wal mode.

Its important that reading the backup is a low priority I/O compared to others.

Eventually we may want to directly copy files from one networked store to another.
- So we would just be
*/

///
pub struct Backup {
    snapshot: Snapshot,

    paths: FilePaths,
    buffer: Vec<u8>,
    files: Vec<NewFileEntry>,
}

impl Backup {
    pub(crate) fn new(version_set: &VersionSet) {

        // Serialize the current manifest as a virtual file
        // ^ We shouldn't serialize any log file numbers though.

        // The assumption is that on disk stuff should already be compressed so
        // we shouldn't need to compress further.

        // Acquire snapshot to retain
        // Might as well re-open all the files and read them from start to end.
        // Eventually should be able to leverage the block cache.
    }
}
