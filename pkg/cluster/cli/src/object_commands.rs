use common::errors::*;
use file::LocalPathBuf;
use cluster_client::ClusterMetaClient;
use cluster_client::meta::table::*;
use cluster_proto::cluster::*;
use cluster_client::id::{entity_id_to_string, entity_id_from_string};
use cluster_client::service::address::ServiceName;
use base_units::ByteCount;
use terminal::TerminalTableBuilder;
use db_table::{query, query_one};

#[derive(Args)]
pub struct GetObjectCommand {
    #[arg(positional)]
    object_name: String,

    #[arg(positional)]
    path: LocalPathBuf
}

#[derive(Args)]
pub struct ListObjectsCommand {
    #[arg(positional)]
    object_prefix: Option<String>
}

#[derive(Args)]
pub struct SetObjectCommand {
    #[arg(positional)]
    object_name: String,

    path: Option<LocalPathBuf>,

    value: Option<String>,

    /// If true, change the object value even if it is already present.
    #[arg(default = false)]
    overwrite: bool,
}

#[derive(Args)]
pub struct DeleteObjectCommand {
    #[arg(positional)]
    object_name: String,
}

pub async fn run_list_object(cmd: ListObjectsCommand) -> Result<()> {
    let meta_client = ClusterMetaClient::create_from_environment().await?;
    let db = meta_client.db();

    // TODO: Don't need to download all the data fields.
    let objs = {
        if let Some(prefix) = &cmd.object_prefix {
            query!(db, ObjectMetadataTable, "STARTS_WITH(name, ?)", prefix)
        } else {
            db.list::<ObjectMetadataTable>().await?
        }
    };

    let mut table = TerminalTableBuilder::new();
    table.row().col("NAME").col("SIZE");

    for obj in objs {
        table.row().col(obj.name()).col(obj.data().len().to_string());
    }

    table.print();

    Ok(())
}

pub async fn run_get_object(cmd: GetObjectCommand) -> Result<()> {
    let meta_client = ClusterMetaClient::create_from_environment().await?;
    let db = meta_client.db();

    let obj = query_one!(db, ObjectMetadataTable, "name = ?", cmd.object_name)
        .ok_or_else(|| err_msg("No such object"))?;

    file::write(&cmd.path, obj.data()).await?;
    Ok(())
}

pub async fn run_set_object(cmd: SetObjectCommand) -> Result<()> {
    let data = {
        if let Some(path) = &cmd.path {
            file::read(path).await?
        } else if let Some(value) = &cmd.value {
            value.as_bytes().to_vec()
        } else {
            return Err(err_msg("Must specify either --path or --value"));
        }
    };
    
    let meta_client = ClusterMetaClient::create_from_environment().await?;
    let db = meta_client.db();
    let mut txn = db.new_transaction().await?;

    let old_obj = query_one!(txn, ObjectMetadataTable, "name = ?", &cmd.object_name);
    if old_obj.is_some() && !cmd.overwrite {
        return Err(err_msg("This object already exists. Use --overwrite to modify it."));
    }

    let mut obj = ObjectMetadata::default();
    obj.set_name(&cmd.object_name);
    obj.set_data(&data[..]);
    txn.put::<ObjectMetadataTable>(&obj).await?;

    txn.commit().await?;

    Ok(())
}

pub async fn run_delete_object(cmd: DeleteObjectCommand) -> Result<()> {
    let meta_client = ClusterMetaClient::create_from_environment().await?;
    let db = meta_client.db();
    let mut txn = db.new_transaction().await?;

    let objects = {
        if let Some(prefix) = cmd.object_name.strip_suffix("*") {
            // TODO: Check if query! works correctly without this if/else
            if prefix.is_empty() {
                txn.list::<ObjectMetadataTable>().await?
            } else {
                query!(txn, ObjectMetadataTable, "STARTS_WITH(name, ?)", prefix)
            }
        } else {
            query!(txn, ObjectMetadataTable, "name = ?", &cmd.object_name)
        }
    };

    println!("Objects to delete:");
    for object in &objects {
        println!("- {}", object.name());
    }

    println!("");
    println!("Continue: [y/N]?");
    if !file::read_user_confirmation().await? {
        return Ok(());
    }

    for object in objects {
        txn.remove::<ObjectMetadataTable>(&object).await?;
    }

    txn.commit().await?;

    Ok(())
}
