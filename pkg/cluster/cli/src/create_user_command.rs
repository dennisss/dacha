use std::sync::Arc;

use common::errors::*;
use common::args::list::CommaSeparated;
use cluster_client::ClusterMetaClient;
use file::LocalPathBuf;
use db_table::db::ProtobufDBTransaction;
use cluster_client::acl::principal::Principal;
use cluster_client::service::address::ServiceName;
use cluster_proto::cluster::*;
use cluster_client::meta::GroupMembershipTable;
use cluster_client::service::create_rpc_channel;
use file::Stdin;
use common::io::Readable;
use terminal::TermiosScope;

#[derive(Args)]
pub struct CreateUserCommand {
    user_name: String,

    groups: CommaSeparated<String>,
}

pub async fn run_create_user(cmd: CreateUserCommand) -> Result<()> {
    let mut meta_client = ClusterMetaClient::create_from_environment().await?;
    let pass = read_stdin_password(true).await?;
    run_create_user_impl(meta_client, &cmd.user_name, &pass, &cmd.groups.values).await?;
    Ok(())
}

pub(crate) async fn read_stdin_password(want_confirmation: bool) -> Result<String> {
    println!("Password: ");
    let pass = read_stdin_pasword_once().await?;

    if want_confirmation {
        println!("Confirm Password: ");
        let confirm_pass = read_stdin_pasword_once().await?;
    
        if pass != confirm_pass {
            return Err(err_msg("Mismatching passwords"));
        }    
    }

    Ok(pass)
}

async fn read_stdin_pasword_once() -> Result<String> {
    let mut stdin = Stdin::get();
    let scope = TermiosScope::no_echo_stdin()?;

    let mut buf = [0u8; 1024];
    let n = stdin.read(&mut buf[..]).await?;

    drop(scope);

    // TODO: Only trim the final '\n' and maybe '\r'
    let value = std::str::from_utf8(&buf[0..n])?.trim();

    Ok(value.into())
}

pub(crate) async fn run_create_user_impl(
    meta_client: Arc<ClusterMetaClient>,
    user_name: &str,
    user_password: &str,
    groups: &[String]
) -> Result<()> {

    let ca_channel = create_rpc_channel(
        "cert-authority.system.job.local.cluster.internal",
        meta_client.clone()
    ).await?;

    let auth_service = UserAuthenticationStub::new(ca_channel);

    {
        let mut req = CreateUserRequest::default();
        req.set_user_name(user_name);
        req.set_user_password(user_password);
        auth_service.CreateUser(&rpc::ClientRequestContext::default(), &req).await.result?;    
    }
    println!("User created!");
    
    let name = ServiceName::for_user(meta_client.zone(), user_name)?;

    if !groups.is_empty() {
        let mut txn = meta_client.db().new_transaction().await?;
        for group_name in groups {
            println!("Adding to group: {}", group_name);
            add_user_to_group(&name, group_name.as_str(), &mut txn).await?;
        }
        txn.commit().await?;    
    }

    println!("Done!");

    Ok(())
}

async fn add_user_to_group(
    name: &ServiceName, group: &str, txn: &mut ProtobufDBTransaction<'_>
) -> Result<()> {
    let mut proto = GroupMembership::default();
    proto.set_group_name(group);
    proto.set_expands(false);
    proto.set_member(Principal::Entity(name.clone()).to_string());
    txn.put::<GroupMembershipTable>(&proto).await
}

