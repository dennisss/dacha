use std::sync::Arc;
use std::collections::HashMap;

use common::errors::*;
use common::hash::FastHasherBuilder;
use net::route::NetworkInterfaceRoute;
use cluster_client::id::{entity_id_to_string, entity_id_from_string};
use mocap_proto::mocap::{CameraStub , CameraSupervisorStub};
use ptp_proto::ptp::TimeSyncStub;

/*
On linux, for testing you can run the following to remove the conneciton:
    nmcli connection delete mocap

*/

pub struct CameraResolver {
    route: NetworkInterfaceRoute,
    client: net::dns::Client,
    iface_description: String,
}

pub struct CameraConnection {
    pub ptp_stub: Arc<TimeSyncStub>,
    pub camera_stub: Arc<CameraStub >,
    pub ptp_addr: String,
    pub rpc_addr: String,
}

impl CameraResolver {

    /// NOTE: In some cases this will fail if we are blocked on setup so will need to be retried later.
    pub async fn create() -> Result<Self> {
        /*
        let config = shared.config.read().await?;

        if config.camera_service().is_empty() {
            return Ok(());
        }

        let resolver = cluster_client::ServiceResolver::create(
            config.camera_service(), shared.meta_client.clone()
        )?;

        drop(config);
        */


        let (route, iface_description) = Self::find_interface().await?;

        let client = net::dns::Client::create_multicast_insecure_with_route(route.clone()).await?;

        Ok(Self {
            route, client, iface_description
        })
    }

    pub fn iface_name(&self) -> String {
        self.route.name.clone()
    }

    pub fn iface_description(&self) -> String {
        self.iface_description.clone()
    }

    pub fn disconnect_missing_cameras(&self) -> bool {
        // With mDNS, we assume there is flakiness so we miss some broadcasts.
        // Should be true if using a clustering system since all cameras will always be in the resolved list.
        false
    }

    // NOTE: DNS currently always waits a total of 1 seconds for this to resolve all cams so
    // this won't return 'fast'
    pub async fn resolve(&mut self) -> Result<HashMap<u64, String, FastHasherBuilder>> {
        /*
        let endpoints = resolver.resolve().await?;
        if endpoints.is_empty() {
            // TODO: Instead rely on the resolver notifications
            executor::sleep(Duration::from_secs(1)).await?;
            continue;
        }

        let mut current_cameras = HashMap::new();
        for endpoint in endpoints {

            let host_name = match &endpoint.authority.host {
                http::uri::Host::Name(v) => v.to_string(),
                _ => return Err(err_msg("Unexpected host name format"))
            };

            let camera_id = match ServiceName::parse(&host_name)?.entity() {
                ServiceEntity::Worker { worker_id, .. } => entity_id_from_string(&worker_id)
                    .ok_or_else(|| err_msg("Failed to parse worker_id format"))?,
                _ => return Err(err_msg("Expected all service endpoints to be workers"))
            };

            current_cameras.insert(camera_id, host_name);
        }
        */

        let mut out = HashMap::default();

        for (addr, ptr) in self.client.resolve_ptr_many("_mocap._tcp.local.").await? {

            // Example PTR string: "camera-h1n1j102va34b._mocap._tcp.local."

            let ptr = match ptr.strip_prefix("camera-") {
                Some(v) => v,
                None => continue
            };
            let ptr = match ptr.strip_suffix("._mocap._tcp.local.") {
                Some(v) => v,
                None => continue
            };

            let id = match entity_id_from_string(&ptr) {
                Some(v) => v,
                None => continue
            };

            out.insert(id, addr.to_string());                
        }

        Ok(out)
    }

    pub async fn connect(&self, endpoint: &str) -> Result<CameraConnection> {

        /*
        // TODO: Ideally we'd just re-use the entire resolved endpoint data.
        // TODO: This also needs to factor in any custom port names.
        let channel = create_rpc_channel(
            &endpoint,
            shared.meta_client.clone()
        ).await?;
        */

        let mut channel_options: rpc::Http2ChannelOptions = format!("http://{}:82", endpoint)
            .as_str()
            .try_into_result()?;
        channel_options.http.backend_balancer.backend.route = Some(self.route.clone());

        let channel = Arc::new(rpc::Http2Channel::create(channel_options).await?);

        let ptp_stub = Arc::new(TimeSyncStub::new(channel.clone()));
        let camera_stub = Arc::new(CameraStub ::new(channel.clone()));

        Ok(CameraConnection {
            ptp_stub,
            camera_stub,
            // rpc_addr: format!("_rpc.{}", endpoint),
            // ptp_addr: format!("_ptp.{}", endpoint),
            rpc_addr: format!("{}:82", endpoint),
            ptp_addr: format!("{}:319", endpoint),
        })
    }

    pub async fn connect_to_supervisor(
        &self, endpoint: &str
    ) -> Result<Arc<CameraSupervisorStub>> {
        let mut channel_options: rpc::Http2ChannelOptions = format!("http://{}:81", endpoint)
            .as_str()
            .try_into_result()?;
        channel_options.http.backend_balancer.backend.route = Some(self.route.clone());

        let channel = Arc::new(rpc::Http2Channel::create(channel_options).await?);

        Ok(Arc::new(CameraSupervisorStub::new(channel.clone())))
    }


    async fn find_interface() -> Result<(NetworkInterfaceRoute, String)> {

        let mut ifaces = net_iface::NetworkInterface::list().await?;

        ifaces.retain(|i| {
            match i.typ {
                net_iface::NetworkInterfaceType::PhysicalEthernet => true,
                _ => false
            }
        });

        let mut first_unconfigured_iface = None; 

        // Try to find an already setup link local interface
        // TODO: Also check that it is 'up'?
        for iface in &ifaces {
            
            let mut has_ipv4 = false;
            for addrs in &iface.addrs {
                let ip = match &addrs.addr {
                    net_iface::NetworkInterfaceAddr::IP(v) => v,
                    _ => continue
                };

                let ipv4 = match &ip {
                    net::ip::IPAddress::V4(v) => v,
                    _ => continue
                };

                has_ipv4 = true;

                if ipv4[0] == 169 && ipv4[1] == 254 {
                    return Ok((NetworkInterfaceRoute {
                        name: iface.name.clone(),
                        addr: ip.clone(),
                        index: iface.index
                    },
                    iface.description.clone())
                    );
                }
            }

            if !has_ipv4 && first_unconfigured_iface.is_none() {
                first_unconfigured_iface = Some(iface.name.clone());
            }
        }

        // Trying to setup the interface.
        #[cfg(target_os = "linux")]
        {
            let iface_name = first_unconfigured_iface
                .ok_or_else(|| err_msg("Could not find any unconfigured interface"))?;

            {
                let show_output = command_args!(
                    "nmcli -t -f NAME,device connection show"
                ).output()?;

                if !show_output.status.success() {
                    return Err(err_msg("Failed to contact NetworkManager"));
                }

                let s = std::str::from_utf8(&show_output.stdout)?;

                for line in s.lines() {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }

                    let (name, dev) = line.split_once(":")
                        .ok_or_else(|| err_msg("Invalid NetworkManager output"))?;
                    
                    if dev == iface_name {
                        return Err(format_err!("Interface '{}' is already configured under connection '{}'. Waiting to get an IP...", iface_name, dev));
                    }
                }

            }

            println!("Newly configuring iface {} for link local addressing...", iface_name);

            let status = command_args!(
                "
                nmcli connection add type ethernet
                    con-name mocap
                    ifname {&iface_name}
                    ipv4.method link-local
                    ipv6.method link-local
                "
                ).status()?;
            if !status.success() {
                return Err(format_err!("Failed to configure {} in NetworkManager", iface_name));
            }

            // We can't immediately return since network manager takes some time to setup the IP.
            return Err(format_err!("Configured '{}'. Waiting for connection...", iface_name));
        }

        Err(err_msg("Failed to find an appropriate ethernet interface"))
    }

}
