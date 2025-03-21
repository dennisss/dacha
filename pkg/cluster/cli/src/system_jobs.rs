use common::errors::*;
use container_proto::cluster::JobSpec;
use file::project_path;
use protobuf::text::ParseTextProto;

pub async fn get_metastore_job(zone: &str) -> Result<JobSpec> {
    JobSpec::parse_text(
        &file::read_to_string(project_path!("pkg/container/config/metastore.job")).await?,
    )
}

pub async fn get_manager_job() -> Result<JobSpec> {
    JobSpec::parse_text(
        &file::read_to_string(project_path!("pkg/container/config/manager.job")).await?,
    )
}

pub async fn get_ca_job() -> Result<JobSpec> {
    JobSpec::parse_text(
        &file::read_to_string(project_path!("pkg/container/config/cert_authority.job")).await?,
    )
}
