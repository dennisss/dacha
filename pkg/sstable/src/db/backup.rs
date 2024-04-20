use std::io::Cursor;
use std::sync::Arc;

use common::errors::*;
use common::io::{Readable, Writeable};
use file::LocalPath;

use crate::db::version::VersionSet;
use crate::db::Snapshot;

use super::paths::FilePaths;
use super::version::Version;
use super::version_edit::NewFileEntry;

/// Serializable (convertable to/from bytes) snapshot of the database at a
/// single point in time (excluding the WAL).
///
/// Internally this wraps the relevant files in a tar archive on the fly.
///
/// TODOs:
/// - Use lower priority file/network reads/writes when processing the
///   backgrounds.
/// - Support just sending references to the files to remote clients (this way
///   if the files are stored in a shared network file system, the remote client
///   can just directly read them rather than asking us).
pub struct Backup {
    /// Version containing all the on-disk tables that we want to back up.
    pub(crate) version: Arc<Version>,

    /// Sequence number of the last write applied to this backup.
    pub(crate) last_sequence: u64,

    /// File number to use for the manifest.
    pub(crate) manifest_number: u64,

    /// Contents of a manifest file which can be describes the files in
    /// 'version'.
    pub(crate) manifest_data: Vec<u8>,

    ///
    pub(crate) log_number: Option<u64>,

    /// Helper to find paths to local files.
    pub(crate) dir: Arc<FilePaths>,
}

impl Backup {
    /// Reads data generated by 'write_to' and dumps it to an empty database
    /// directory.
    ///
    /// If this operation is successful, then the directory can be loaded with
    /// the normal EmbeddedDB functions.
    pub async fn read_from<R: Readable>(reader: R, output_dir: &LocalPath) -> Result<()> {
        let mut archive = compression::tar::Reader::new(reader);
        archive.extract_files(output_dir).await?;
        Ok(())
    }

    pub fn last_sequence(&self) -> u64 {
        self.last_sequence
    }

    /// Serializes the backup to a byte stream.
    pub async fn write_to(&self, writer: &mut dyn Writeable) -> Result<()> {
        let mut archive = compression::tar::Writer::new(writer);

        for level in &self.version.levels {
            for table in &level.tables {
                let local_path = self.dir.table(table.entry.number);
                let output_path = local_path.strip_prefix(self.dir.root_dir()).unwrap();

                let mut file = file::LocalFile::open(&local_path)?;

                archive
                    .append_regular_file(output_path.as_str(), table.entry.file_size, &mut file)
                    .await?;
            }
        }

        let manifest_path = self
            .dir
            .manifest(self.manifest_number)
            .strip_prefix(self.dir.root_dir())
            .unwrap()
            .to_owned();

        archive
            .append_regular_file(
                manifest_path.as_str(),
                self.manifest_data.len() as u64,
                &mut Cursor::new(&self.manifest_data),
            )
            .await?;

        let current_file_data = format!("{}\n", manifest_path.as_str());

        archive
            .append_regular_file(
                "CURRENT",
                current_file_data.len() as u64,
                &mut Cursor::new(current_file_data.as_bytes()),
            )
            .await?;

        archive
            .append_regular_file("LOCK", 0, &mut Cursor::new(&[]))
            .await?;

        if let Some(log_num) = &self.log_number {
            let log_path = self
                .dir
                .log(*log_num)
                .strip_prefix(self.dir.root_dir())
                .unwrap()
                .to_owned();

            // Only works as 0 bytes is a valid log.
            archive
                .append_regular_file(log_path.as_str(), 0, &mut Cursor::new(&[]))
                .await?;
        }

        // TODO: Also copy IDENTITY

        archive.finish().await?;

        Ok(())
    }

    /// Quickly estimates how large the backup generated by write_to() will be.
    pub async fn approximate_size(&self) -> Result<u64> {
        let mut size = 0;

        for level in &self.version.levels {
            for table in &level.tables {
                let local_path = self.dir.table(table.entry.number);

                let mut file = file::LocalFile::open(&local_path)?;

                size += file.metadata().await?.len();

                // Tar header block size.
                size += 512;
            }
        }

        size += self.manifest_data.len() as u64;

        // Tar header block size for MANIFEST, CURRENT, and LOCK file.
        size += 512 * 3;

        Ok(size)
    }
}
