use common::errors::*;

use sstable::record_log::RecordReader;
use file::{project_path, LocalPath};
use mocap_proto::mocap::*;
use protobuf::{StaticMessage, Message};
use protobuf_json::MessageJsonSerialize;
use math::matrix::axis_angle::*;
use math::matrix::{vec2f, vec3f, Vector2f, Matrix3f, Vector3f, Matrix4f};
use vision::{CameraIntrinsicsModel, CameraExtrinsics};


pub async fn read_log_file(path: &LocalPath) -> Result<Vec<MocapLogEntry>> {
    let mut reader = RecordReader::open(path).await?;
    
    let mut entries = vec![];
    while let Some(record) = reader.read().await? {
        entries.push(MocapLogEntry::parse(&record)?);
    }

    Ok(entries)
}

pub fn intrinsics_from_proto(proto: &CameraIntrinsicsModelProto) -> CameraIntrinsicsModel {
    CameraIntrinsicsModel {
        focal_length: Vector2f::from_slice(proto.focal_length()),
        center: Vector2f::from_slice(proto.center()),
        k1: proto.k1(),
        k2: proto.k2()
    }
}

pub fn extrinsics_to_proto(v: &CameraExtrinsics) -> CameraExtrinsicsProto {
    let mut proto = CameraExtrinsicsProto::default();
    proto.rotation_mut().extend_from_slice(v.rotation.as_ref());
    proto.translation_mut().extend_from_slice(v.translation.as_ref());
    proto
}

pub fn extrinsics_from_proto(proto: &CameraExtrinsicsProto) -> CameraExtrinsics {
    CameraExtrinsics {
        rotation: Vector3f::from_slice(proto.rotation()),
        translation: Vector3f::from_slice(proto.translation()),
    }
}