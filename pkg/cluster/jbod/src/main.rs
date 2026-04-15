#[macro_use]
extern crate macros;
#[macro_use]
extern crate common;

use std::sync::Arc;
use std::collections::HashSet;
use std::time::Duration;

use base_util::InRange;
use base_error::*;
use file::LocalPathBuf;
use peripherals_service::mcp23008::*;
use peripherals_service::device::PeripheralsDevice;
use cluster_jbod::management::*;
use cluster_jbod_proto::cluster::*;
use common::io::Readable;
use executor_multitask::RootResource;
use rpc_util::NamedPortArg;
use cluster_client::ClusterMetaClient;

const SERVICE_ACL_PROTO: &'static str = r#"
    rules: [
        {
            path: "/"
            is_directory: false
            principals: ["authenticated"]
        },
        {
            path: "/rpc/cluster.Enclosure"
            is_directory: true
            principals: ["group:cluster-owners"]
        }
    ]
"#;

#[derive(Args)]
struct Args {
    port: NamedPortArg,

    #[arg(default = false)]
    dummy: bool,
}

const TEST_STATE_PROTO: &'static str = r#"

    fan_groups: [
        {
            duty_cycle: 0.7
            fans: [
                {
                    name: "front_left"
                    measured_speed: 2637.2393
                },
                {
                    name: "back_left"
                    measured_speed: 3236.6147
                },
                {
                    name: "front_middle"
                    measured_speed: 2757.1199
                },
                {
                    name: "back_middle"
                    measured_speed: 3206.6216
                },
                {
                    name: "front_right"
                },
                {
                    name: "back_right"
                    measured_speed: 2127.953
                }
            ]
        }
    ]
    bays: [
        {},
        {},
        {},
        {},
        {},
        {},
        {},
        {
            connected_device_name: "sdd"
        },
        {},
        {},
        {},
        {},
        {},
        {},
        {},
        {},
        {},
        {
            connected_device_name: "sda"
        },
        {},
        {
            connected_device_name: "sde"
        },
        {},
        {},
        {},
        {},
        {},
        {},
        {},
        {},
        {},
        {},
        {},
        {},
        {
            connected_device_name: "sdb"
        },
        {},
        {
            connected_device_name: "sdc"
        },
        {},
        {},
        {},
        {},
        {},
        {},
        {},
        {},
        {},
        {}
    ]
    psus: [
        {
            name: "left"
            on: true
            output_stable: true
            voltage_ps_on: 0.23291017
            voltage_5: 5.051148
            voltage_12: 11.171931
            sas_on: true
        },
        {
            name: "right"
            voltage_ps_on: -0.0029296877
            voltage_5: -0.0021972659
            voltage_12: -0.0089355465
        }
    ]
    storage_devices: [
        {
            name: "expander-1:1"
            model: "INTEL RES3FV288 (B022)"
            usage: EXPANDER
            temperature: 50
            position: "left"
            wwid: "0x50000d1702582cbf"
        },
        {
            name: "sda"
            model: "WUH721818AL5204"
            parent: "expander-1:1"
            usage: DISK
            temperature: 32
            serial_number: "3JGV1ANG"
            wwid: "0x5000cca2a92f5ac5"
            disk_stats {
                smart_status: "OK"
            }
        },
        {
            name: "sdb"
            model: "WUH721818AL5204"
            parent: "expander-1:1"
            usage: DISK
            temperature: 31
            serial_number: "3WG2Z36K"
            wwid: "0x5000cca2840566fd"
            disk_stats {
                smart_status: "OK"
                read_soft_errors: 2
            }
        },
        {
            name: "sdc"
            model: "WUH721818AL5204"
            parent: "expander-1:1"
            usage: DISK
            temperature: 30
            serial_number: "4BJ87PRV"
            wwid: "0x5000cca2b67fbb89"
            disk_stats {
                smart_status: "OK"
                read_soft_errors: 37
            }
        },
        {
            name: "sdd"
            model: "WUH721818AL5204"
            parent: "expander-1:1"
            usage: DISK
            temperature: 29
            serial_number: "4BK2W06Y"
            wwid: "0x5000cca2b6ae5289"
            disk_stats {
                smart_status: "OK"
            }
        },
        {
            name: "sde"
            model: "WUH721818AL5204"
            parent: "expander-1:1"
            usage: DISK
            temperature: 28
            serial_number: "2JGG6D8C"
            wwid: "0x5000cca2ab19d4d1"
            disk_stats {
                smart_status: "OK"
            }
        }
    ]

"#;


pub struct TestEnclosureServiceInst {}

#[async_trait]
impl EnclosureService for TestEnclosureServiceInst {
    async fn GetState(
        &self,
        request: rpc::ServerRequest<GetStateRequest>,
        response: &mut rpc::ServerResponse<EnclosureState>,
    ) -> Result<()> {
        protobuf::text::parse_text_proto(TEST_STATE_PROTO, &mut response.value)
    }

    async fn Execute(
        &self,
        request: rpc::ServerRequest<ExecuteRequest>,
        response: &mut rpc::ServerResponse<ExecuteResponse>,
    ) -> Result<()> {
        Err(err_msg("Not implemented"))
    }
}


#[executor_main]
async fn main() -> Result<()> {
    println!("Starting...");

    let args = common::args::parse_args::<Args>()?;

    let service = RootResource::new();

    let client = ClusterMetaClient::create_from_environment().await?;
    service.register_dependency(client.clone()).await;
    
    let mut acl = cluster_proto::cluster::ServiceACLProto::default();
    protobuf::text::parse_text_proto(SERVICE_ACL_PROTO, &mut acl)?;

    let mut server = cluster_client::ClusterServer::new(args.port.value(), acl, client)?;

    if args.dummy {
        let inst = Arc::new(TestEnclosureServiceInst {});
        server.add_service(inst.clone().into_service())?;
    } else {
        let inst = Arc::new(cluster_jbod::service::EnclosureServiceInst::create().await?);
        service.register_dependency(inst.clone()).await;
        server.add_service(inst.clone().into_service())?;
    }

    let web_handler = Arc::new(web::WebPageHandler::create(web::WebPageOptions {
        title: "JBOD Enclosure".into(),
        script_path: "built/pkg/cluster/jbod/app.js".into(),
        vars: None,
    }).await?);
    server.add_request_handler("/", false, web_handler.clone())?;

    service.register_dependency(server.start()?).await;

    println!("Ready!");

    service.wait().await
}