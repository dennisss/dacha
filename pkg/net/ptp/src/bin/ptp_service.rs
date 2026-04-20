/*

cargo run --bin cluster_cli -- start_job pkg/net/ptp/config/ptp_follower.job

cargo run --bin cluster_cli -- list workers

cargo run --bin cluster_cli -- log --worker_name=ptp_follower.amv5hvhdjxktg --latest_attempt




cargo run --bin ptp_service -- \
    --mode=leader \
    --rpc_port=8002 \
    --ptp_port=8003 \
    --iface=enp5s0 \
    --follower_rpc_addrs="_rpc.ptp_follower.job.local.cluster.internal" \
    --follower_ptp_addrs="_ptp.ptp_follower.job.local.cluster.internal"
*/

#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use std::time::Duration;
use std::sync::Arc;

use common::args::list::CommaSeparated;
use common::errors::*;
use executor_multitask::RootResource;
use rpc_util::NamedPortArg;
use ptp_proto::ptp::*;
use cluster_client::{ClusterServer, ClusterMetaClient};


const SERVICE_ACL_PROTO: &'static str = r#"
    rules: [
        {
            path: "/rpc/ptp.TimeSync"
            is_directory: true
            principals: ["authenticated"]
        }
    ]
"#;

const DEFAULT_CONFIG_PROTO: &'static str = r#"
    leader {
        sync_interval_seconds: 0.25
        followers: []
        ptp_timeout_seconds: 0.01
        sync_timeout_seconds: 0.01
        realtime_clock_sync {
            p_scale: 0.01
            i_scale: 0.0001
            max_offset_seconds: 1
            max_error_integral_secs: 0.05
            max_correction_ppm: 50
        }
    }

    follower {
        leader_clock_sync {
            p_scale: 0.5
            i_scale: 0.05
            max_offset_seconds: 0.0001 # 100us
            max_error_integral_secs: 0.05

            # Must be larger than the leader -> ntp one
            max_correction_ppm: 100
        }
        max_network_rtt: 0.00001 # 10us
        max_sample_age: 0.1
    }

"#;


#[derive(Args)]
struct Args {
    rpc_port: NamedPortArg,

    ptp_port: NamedPortArg,

    mode: Mode,

    iface: String,

    follower_rpc_addrs: CommaSeparated<String>,

    follower_ptp_addrs: CommaSeparated<String>,
}

#[derive(Args)]
pub enum Mode {
    #[arg(name = "leader")]
    Leader,
    #[arg(name = "follower")]
    Follower
}

#[executor_main]
async fn main() -> Result<()> {

    let args = common::args::parse_args::<Args>()?;

    let mut ptp_config = TimeSyncConfig::default();
    protobuf::text::parse_text_proto(DEFAULT_CONFIG_PROTO, &mut ptp_config)?;

    match args.mode {
        Mode::Leader => {
            ptp_config.set_role(TimeSyncRole::LEADER);
            
            if args.follower_rpc_addrs.values.len() != args.follower_ptp_addrs.values.len() {
                return Err(err_msg("Expected the same number of follower ptp and rpc addresses"));
            }

            for i in 0..args.follower_rpc_addrs.values.len() {
                let proto = ptp_config.leader_mut().new_followers();
                proto.set_rpc_addr(&args.follower_rpc_addrs.values[i]);
                proto.set_ptp_addr(&args.follower_ptp_addrs.values[i]);
            }
        }
        Mode::Follower => {
            ptp_config.set_role(TimeSyncRole::FOLLOWER);
        }
    }

    let service = RootResource::new();

    let client = ClusterMetaClient::create_from_environment().await?;
    service.register_dependency(client.clone()).await;

    let mut acl = cluster_proto::cluster::ServiceACLProto::default();
    protobuf::text::parse_text_proto(SERVICE_ACL_PROTO, &mut acl)?;

    let mut server = ClusterServer::new(args.rpc_port.value(), acl, client.clone())?;

    println!("RPC Port: {}", args.rpc_port.value());
    println!("PTP Port: {}", args.ptp_port.value());

    // TODO: Need to ensure that the device matches the interface.
    let ptp_device = Arc::new(ptp::PTPDevice::open_default()?);
    let ptp_socket = ptp::TimestampedUdpSocket::create(
        format!("0.0.0.0:{}", args.ptp_port.value()).parse()?,
        &args.iface
    ).await?;


    let inst = Arc::new(ptp::TimeSyncNode::create(client.clone(), ptp_device, ptp_socket).await);
    service.register_dependency(inst.clone()).await;
    server.add_service(inst.clone().into_service())?;

    inst.configure(&ptp_config).await?;

    service.register_dependency(server.start()?).await;


    println!("Running...");
    service.wait().await
}