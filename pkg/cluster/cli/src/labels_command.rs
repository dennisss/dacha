use cluster_client::meta::{client::ClusterMetaClient, NodeSchedulingMetadataTable};
use common::errors::*;
use cluster_proto::cluster::Labels;
use db_table::query_one;
use protobuf::Message;
use cluster_client::id::entity_id_from_string;

/// Maximum combined serialized size of all custom labels associated with a
/// single node.
const MAX_LABELS_SIZE: usize = 4096;

// NOTE: Each key/value must contain at least one character.
// Delimiters like '=' and ':' are reserved to allow for selector strings.
regexp!(LABEL_DATA_PATTERN => "^[a-z0-9_\\-\\.:=]+$");

#[derive(Args)]
pub struct LabelsCommand {
    sub_command: LabelsSubCommand,

    node_id: String,
}

#[derive(Args)]
enum LabelsSubCommand {
    #[arg(name = "get")]
    Get,

    #[arg(name = "set")]
    Set(SetLabelsCommand),
}

#[derive(Args)]
struct SetLabelsCommand {
    #[arg(positional)]
    values: String,

    #[arg(default = false)]
    override_all: bool,
}

fn validate_labels(labels: &Labels) -> Result<()> {
    if labels.serialize()?.len() > MAX_LABELS_SIZE {
        return Err(rpc::Status::invalid_argument("Labels are too large").into());
    }

    for label in labels.label() {
        if !LABEL_DATA_PATTERN.test(label.key().as_bytes())
            || !LABEL_DATA_PATTERN.test(label.value().as_bytes())
        {
            return Err(
                rpc::Status::invalid_argument("Labels contain non-allowed characters.").into(),
            );
        }
    }

    Ok(())
}

pub async fn run_labels(cmd: LabelsCommand) -> Result<()> {
    let node_id = entity_id_from_string(&cmd.node_id).ok_or_else(|| err_msg("Invalid --node_id"))?;

    let meta_client = ClusterMetaClient::create_from_environment().await?;
    let db = meta_client.db();

    let mut txn = db.new_transaction().await?;

    let mut node_meta = query_one!(txn, NodeSchedulingMetadataTable, "node_id = ?", node_id)
        .unwrap_or_default();
    node_meta.set_node_id(node_id);

    match cmd.sub_command {
        LabelsSubCommand::Get => {}
        LabelsSubCommand::Set(sub_cmd) => {
            if sub_cmd.override_all {
                node_meta.clear_labels();
            }

            for part in sub_cmd.values.split(",") {
                let (k, v) = part
                    .split_once("=")
                    .ok_or_else(|| err_msg("Missing : delimiter in label string"))?;

                // Delete any existing entry.
                node_meta.labels_mut().label_mut().retain(|l| l.key() != k);

                if !v.is_empty() {
                    let l = node_meta.labels_mut().new_label();
                    l.set_key(k);
                    l.set_value(v);
                }
            }

            validate_labels(node_meta.labels())?;

            txn.put::<NodeSchedulingMetadataTable>(&node_meta).await?;
            txn.commit().await?;
        }
    }

    println!("Node Labels:\n{:?}", node_meta.labels());

    Ok(())
}
