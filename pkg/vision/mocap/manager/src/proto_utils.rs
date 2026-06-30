use common::errors::*;

use sstable::record_log::RecordReader;
use file::{project_path, LocalPath};
use mocap_proto::mocap::*;
use protobuf::{StaticMessage, Message};
use protobuf_json::MessageJsonSerialize;
use math::matrix::axis_angle::*;
use math::matrix::{vec2d, vec3d, Vector2d, Matrix3d, Vector3d};
use vision::{CameraIntrinsicsModel, CameraExtrinsics};


pub async fn read_log_file(path: &LocalPath) -> Result<Vec<MocapLogEntry>> {
    let mut reader = RecordReader::open(path).await?;
    
    let mut entries = vec![];
    while let Some(record) = reader.read().await? {
        entries.push(MocapLogEntry::parse(&record)?);
    }

    Ok(entries)
}
