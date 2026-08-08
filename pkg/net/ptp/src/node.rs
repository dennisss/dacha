use std::sync::Arc;
use std::time::{Duration, Instant};
use std::collections::{HashMap, HashSet};

use common::errors::*;
use executor_multitask::{impl_resource_passthrough, ServiceResourceGroup};
use ptp_proto::ptp::*;
use net::ip::SocketAddr;
use executor::{lock_async, lock};
use executor::sync::AsyncMutex;
use crypto::random::SharedRng;
use cluster_client::{ClusterMetaClient, ServiceResolver};
use protobuf::Message;
use http::Resolver;
use executor::sync::AsyncVariable;

use crate::signed_duration::*;
use crate::socket::*;
use crate::device::*;

const ID_SIZE: usize = 8;



pub struct TimeSyncNode {
    resources: ServiceResourceGroup,
    shared: Arc<Shared>
}

impl_resource_passthrough!(TimeSyncNode, resources);

struct Shared {
    meta_client: Arc<ClusterMetaClient>,
    ptp_device: Arc<PTPDevice>,
    ptp_socket: TimestampedUdpSocket,
    state: AsyncVariable<State>,
}

#[derive(Default)]
struct State {
    config: TimeSyncConfig,

    follower_state: Option<FollowerState>,

    // TODO: Probably keep one record per peer address so that we gracefully support leadership changes.
    last_rx: Option<ReceivedPacket>,
    last_tx: Option<SentPacket>,
}

#[derive(Default)]
struct FollowerState {
    pi_filter: PIFilterState,
    last_sync: Option<LastSyncMetadata>,
}

struct LastSyncMetadata {
    time: Instant,
    rtt: f64,
    error: f64,
}

struct ReceivedPacket {
    time: Instant,
    peer_addr: SocketAddr,
    rx_id: Vec<u8>,
    rx_time: u64,
}

struct SentPacket {
    tx_id: Vec<u8>,
    tx_time: u64,
}

enum BackgroundState {
    None,
    Leader(LeaderState)
}

#[derive(Default)]
struct LeaderState {
    last_round: Option<Instant>,
    
    // The key is the serialized protos.
    followers: HashMap<Vec<u8>, LeaderPeer>,

    pi_filter: PIFilterState
}

#[derive(Default)]
struct PIFilterState {
    integral: f64,
    last_time: Option<Instant>,
}

struct LeaderPeer {
    proto: TimeSyncPeerAddr,
    configured: bool,
    rpc: Arc<TimeSyncStub>,
    ptp: ServiceResolver
}

impl TimeSyncNode {
    pub fn default_config() -> TimeSyncConfig {
        ptp_core::default_config()
    }

    pub async fn create(
        meta_client: Arc<ClusterMetaClient>,
        ptp_device: Arc<PTPDevice>,
        ptp_socket: TimestampedUdpSocket
    ) -> Self {

        let shared = Arc::new(Shared {
            meta_client,
            ptp_device,
            ptp_socket,
            state: AsyncVariable::default()
        });

        let mut resources = ServiceResourceGroup::new("TimeSyncNode");

        resources.spawn_interruptable(
            "TimeSyncNode::background_thread", Self::background_thread(shared.clone())
        ).await;
        resources.spawn_interruptable(
            "TimeSyncNode::server_thread", Self::server_thread(shared.clone())
        ).await;

        Self {
            shared,
            resources,
        }
    }

    pub async fn status(&self) -> Result<StatusResponse> {
        lock!(state <= self.shared.state.lock().await?, {
            let mut out = StatusResponse::default();
            out.set_config(state.config.clone());
            if let Some(follower_state) = &state.follower_state {
                let proto = out.follower_mut();
                
                if let Some(last_sync) = &follower_state.last_sync {
                    proto.set_got_sync(true);
                    proto.set_last_leader_error(last_sync.error);
                    proto.set_last_leader_rtt(last_sync.rtt);
                    proto.set_last_sync_age((Instant::now() - last_sync.time).as_secs_f64());
                }
            }

            Ok(out)
        })
    }

    pub async fn configure(&self, config: &TimeSyncConfig) -> Result<()> {
        lock!(state <= self.shared.state.lock().await?, {
            state.config = config.clone();

            if config.role() == TimeSyncRole::FOLLOWER {
                state.follower_state = Some(FollowerState::default());
            } else {
                state.follower_state = None;
            }

            Ok(())
        })
    }

    pub async fn sync(&self, point: &TimeSyncPoint) -> Result<()> {
        Self::sync_impl(&self.shared, point).await
    }

    async fn sync_impl(shared: &Shared, remote_point: &TimeSyncPoint) -> Result<()> {
        lock!(state <= shared.state.lock().await?, {
            let state = &mut *state;

            let follower_state = state.follower_state.as_mut()
                .ok_or_else(|| err_msg("Not currently a follower"))?;

            let local_rx = state.last_rx.take()
                .ok_or_else(|| err_msg("No PTP packet received recently"))?;
            if &local_rx.rx_id != remote_point.tx_id() {
                return Err(err_msg("Local RX / remote TX mismatch"));
            }

            let local_tx = state.last_tx.take()
                .ok_or_else(|| err_msg("No PTP packet send recently"))?;
            if &local_tx.tx_id != remote_point.rx_id() {
                return Err(err_msg("Local TX / remote RX mismatch"));
            }

            let remote_time_elapsed = remote_point.rx_time() - remote_point.tx_time();
            let local_time_elapsed = local_tx.tx_time - local_rx.rx_time;

            let rtt = {
                if remote_time_elapsed >= local_time_elapsed {
                    remote_time_elapsed - local_time_elapsed
                } else {
                    eprintln!("Near zero PTP RTT!");
                    0
                }
            };

            let rtt = Duration::from_nanos(rtt);

            if rtt > Duration::from_secs_f32(state.config.follower().max_network_rtt()) {
                return Err(format_err!("Sync RTT is too long: {:?}", rtt));
            }

            let mut local_time = Duration::from_nanos(local_rx.rx_time);
            let mut remote_time = Duration::from_nanos(remote_point.tx_time()) + (rtt / 2);

            // Advance the times to the current point in time.
            // TODO: Base this on the PTP clock?
            {
                let now: Duration = shared.ptp_device.get_time()?.into();
                let elapsed_time = now - local_time;
                if elapsed_time > Duration::from_secs_f32(state.config.follower().max_sample_age()) {
                    return Err(format_err!("Sync sample age ({:?}) is too old", elapsed_time));
                }

                // local_time += elapsed_time;
                // remote_time += elapsed_time;
            }

            let error = SignedDuration::from(remote_time) - SignedDuration::from(local_time);

            // println!("Sync Error: {:?}; RTT: {:?}", error, rtt);

            Self::run_time_adjustment(
                shared,
                remote_time,
                error.as_secs_f64(),
                state.config.follower().leader_clock_sync(),
                &mut follower_state.pi_filter
            )?;

            follower_state.last_sync = Some(LastSyncMetadata {
                time: local_rx.time,
                error: error.as_secs_f64(),
                rtt: rtt.as_secs_f64()
            });

            Ok(())
        })
    }

    async fn background_thread(shared: Arc<Shared>) -> Result<()> {
        let mut background_state = BackgroundState::None;

        loop {
            let config = lock!(state <= shared.state.lock().await?, {
                state.config.clone()
            });


            match config.role() {
                TimeSyncRole::LEADER => {
                    match &background_state {
                        BackgroundState::Leader(_) => {},
                        _ => {
                            background_state = BackgroundState::Leader(LeaderState::default());
                        }
                    }

                    let state = match &mut background_state {
                        BackgroundState::Leader(v) => v,
                        _ => panic!()
                    };

                    Self::run_leader_cycle(&shared, &config, state).await?;
                }
                _ => {
                    background_state = BackgroundState::None;
                }
            }

            // TODO: Add random jitter to this.
            executor::sleep(Duration::from_millis(100)).await?;
        }


    }

    async fn run_leader_cycle(
        shared: &Arc<Shared>,
        config: &TimeSyncConfig,
        leader_state: &mut LeaderState
    ) -> Result<()> {

        let now = Instant::now();

        if let Some(last_time) = &leader_state.last_round {
            if (now - *last_time) < Duration::from_secs_f32(config.leader().sync_interval_seconds()) {
                return Ok(());
            }
        }
        leader_state.last_round = Some(now);

        Self::run_leader_ntp_sync(shared, config, leader_state).await?;

        Self::run_leader_all_followers_sync(shared, config, leader_state).await?;

        Ok(())
    }

    async fn run_leader_ntp_sync(
        shared: &Shared,
        config: &TimeSyncConfig,
        leader_state: &mut LeaderState
    ) -> Result<()> {
        // TODO: Use a better syscall for capturing both of these times simultaneously.
        let ptp_time_raw = shared.ptp_device.get_time()?;
        let ptp_time: Duration = ptp_time_raw.into();
        let real_time_raw = sys::ClockId::REALTIME.get_time()?;
        let real_time: Duration = real_time_raw.into();

        let error = SignedDuration::from(real_time) - SignedDuration::from(ptp_time);

        // println!("NTP Error: {:?}", error);

        Self::run_time_adjustment(
            shared,
            real_time,
            error.as_secs_f64(),
            config.leader().realtime_clock_sync(),
            &mut leader_state.pi_filter
        )?;

        Ok(())
    }

    fn run_time_adjustment(
        shared: &Shared,
        remote_time: Duration,
        mut error: f64,
        config: &TimeAdjustmentConfig,
        state: &mut PIFilterState,
    ) -> Result<()> {
        if error.abs() > config.max_offset_seconds() {
            shared.ptp_device.clock().set_offset_secs_f64(error)?;
            // println!("=> time shifted!");

            *state = PIFilterState::default();
            error = 0.0;
        }

        let now = Instant::now();
        if let Some(last_time) = &state.last_time {
            // TODO: Disallow large jumps between syncs.

            let dt = (now - *last_time).as_secs_f64();
            state.integral += dt * error;
            state.integral = clamp_magnitude(state.integral, config.max_error_integral_secs());
        }
        state.last_time = Some(now);

        let mut out = config.p_scale() * error +
            config.i_scale() * state.integral;
        out *= 1_000_000.0; // scale to ppm.

        out = clamp_magnitude(out, config.max_correction_ppm());

        // println!("=> freq: {:.2}ppm", out);

        out *= (1 << 16) as f64;

        let mut adj = shared.ptp_device.clock().get_adjustments()?;
        adj.set_freq(out.round() as i64);
        shared.ptp_device.clock().set_adjustments(&adj)?;

        Ok(())
    }

    async fn run_leader_all_followers_sync(
        shared: &Arc<Shared>,
        config: &TimeSyncConfig,
        leader_state: &mut LeaderState
    ) -> Result<()> {
        // Get rid of any old follower entries.
        {
            let mut follower_set = HashSet::<Vec<u8>>::new();
            for follower in config.leader().followers() {
                follower_set.insert(follower.serialize()?);
            }

            let mut old_followers = vec![];

            for key in leader_state.followers.keys() {
                if !follower_set.contains(key) {
                    old_followers.push(key.clone());
                }
            }

            for key in old_followers  {
                leader_state.followers.remove(&key);
            }
        }

        // Add new follower entries.

        for follower in config.leader().followers() {
            let follower_key = follower.serialize()?;
            if leader_state.followers.contains_key(&follower_key) {
                continue;
            }

            let channel = cluster_client::service::create_rpc_channel(
                &follower.rpc_addr(),
                shared.meta_client.clone()
            ).await?;

            let rpc = Arc::new(TimeSyncStub::new(channel));

            let ptp = cluster_client::ServiceResolver::create(
                follower.ptp_addr(), shared.meta_client.clone()
            )?;

            leader_state.followers.insert(follower_key, LeaderPeer {
                proto: follower.as_ref().clone(),
                configured: false,
                rpc,
                ptp
            });
        }

        // Sync to all peers.
        // - Individual peer failures shouldn't take down the whole leader.
        // - These can't be parallelized since that would risk increasing network queueing and thus RTT

        for follower in leader_state.followers.values_mut() {
            if let Err(e) = Self::run_leader_one_follower_sync(shared, config, follower).await {
                eprintln!("Failed to sync to: {}: {}", follower.proto.rpc_addr(), e);
            }
        }

        Ok(())
    }

    async fn run_leader_one_follower_sync(
        shared: &Arc<Shared>,
        config: &TimeSyncConfig,
        follower: &mut LeaderPeer
    ) -> Result<()> {
        let request_context = rpc::ClientRequestContext::default();

        if !follower.configured {
            let mut req = ConfigureRequest::default();
            req.set_config(config.clone());
            req.config_mut().set_role(TimeSyncRole::FOLLOWER);

            let _ = executor::timeout(
                Duration::from_secs_f32(config.leader().sync_timeout_seconds()),
                follower.rpc.Configure(&request_context, &req)
            )
            .await
            .map_err(|_| err_msg("Timed out on Configure RPC"))?
            .result?;

            follower.configured = true;

            println!("Configured!");
        }

        let ptp_endpoints = follower.ptp.resolve().await?;
        if ptp_endpoints.is_empty() {
            return Err(err_msg("No PTP addresses resolved"));
        }

        if ptp_endpoints.len() != 1 {
            return Err(err_msg("More than 1 PTP address resolved"));
        }

        let ptp_addr = &ptp_endpoints[0].address;

        // println!("Sending to: {:?}", ptp_addr);

        // Clear any old data so that 'Self::get_received_packet' only notices newly
        // received data.
        lock!(state <= shared.state.lock().await?, {
            state.last_tx = None;
            state.last_rx = None;
        });

        let mut tx_id = vec![0u8; ID_SIZE];
        crypto::random::global_rng().generate_bytes(&mut tx_id).await;

        let tx_time = shared.ptp_socket.send_to(&tx_id, ptp_addr).await?;

        let rx_packet = executor::timeout(
            Duration::from_secs_f32(config.leader().ptp_timeout_seconds()),
            Self::get_received_packet(shared.clone())
        )
        .await
        .map_err(|_| err_msg("Timed out while receiving PTP response"))??;

        if rx_packet.peer_addr != *ptp_addr {
            return Err(format_err!("Received PTP response from wrong peer: {:?} vs {:?}",
                rx_packet.peer_addr, ptp_addr));
        }

        let mut point = TimeSyncPoint::default();
        point.set_tx_id(tx_id);
        point.set_tx_time(tx_time);
        point.set_rx_id(rx_packet.rx_id);
        point.set_rx_time(rx_packet.rx_time);


        let _ = executor::timeout(
            Duration::from_secs_f32(config.leader().sync_timeout_seconds()),
            follower.rpc.Sync(&request_context, &point)
        )
        .await
        .map_err(|_| err_msg("Timed out on Sync RPC"))?
        .result?;

        Ok(())
    }

    async fn get_received_packet(shared: Arc<Shared>) -> Result<ReceivedPacket> {
        loop {
            let mut state = shared.state.lock().await?.enter();
            if let Some(pkt) = state.last_rx.take() {
                state.exit();
                return Ok(pkt);
            }

            state.wait().await;
        }
    }

    // Receives PTP packets from peers and stores them in last_rx
    // If we are a follower, this also handles responding to the packets and stores that data in last_tx
    //
    // NOTE: the ptp_socket binds to the ethernet device so all packets sent/received should have
    // timestamps if the local machine is healthy.
    async fn server_thread(shared: Arc<Shared>) -> Result<()> {

        let mut buf = [0u8; 32];
        
        loop {
            // TODO: Don't error out if an invalid packet type is received
            let (n_received, rx_time, peer_addr) = shared.ptp_socket.recv_from(&mut buf).await?;
            if n_received != ID_SIZE {
                continue;
            }
            

            let time = Instant::now();

            // TODO: Reject if not received from the leader address.

            let rx_id = buf[0..n_received].to_vec();

            // println!("RX: {:?}: {:?}", peer_addr, rx_id);

            // Locking before the transfer to ensure that the last_rx state is updated before the state
            // is visible to sync().
            lock_async!(state <= shared.state.lock().await?, {
                // Drop extra packets until the current one is acknowledged.
                // NOTE: we do need to allow overwriting old packets since a leader may fail before the sync RPC is called.
                // if state.last_rx.is_some() {
                //     return Ok(());
                // }
                
                state.last_rx = Some(ReceivedPacket {
                    time,
                    peer_addr: peer_addr.clone(),
                    rx_id,
                    rx_time,
                });
                
                if state.config.role() == TimeSyncRole::FOLLOWER {
                    let mut tx_id = vec![0u8; ID_SIZE];
                    crypto::random::global_rng().generate_bytes(&mut tx_id).await;

                    let tx_time = shared.ptp_socket.send_to(&tx_id, &peer_addr).await?;

                    state.last_tx = Some(SentPacket {
                        tx_id,
                        tx_time,
                    });
                } else {
                    state.last_tx = None;
                }

                state.notify_all();

                Result::<_, Error>::Ok(())
            })?;
        }
    }
}

#[async_trait]
impl TimeSyncService for TimeSyncNode {

    async fn Status(
        &self,
        request: rpc::ServerRequest<StatusRequest>,
        response: &mut rpc::ServerResponse<StatusResponse>
    ) -> Result<()> {
        response.value = self.status().await?;
        Ok(())
    }

    async fn Configure(
        &self,
        request: rpc::ServerRequest<ConfigureRequest>,
        response: &mut rpc::ServerResponse<ConfigureResponse>
    ) -> Result<()> {
        self.configure(request.config()).await?;
        Ok(())
    }

    async fn Sync(
        &self,
        request: rpc::ServerRequest<TimeSyncPoint>,
        response: &mut rpc::ServerResponse<SyncResponse>
    ) -> Result<()> {
        self.sync(&request.value).await?;
        Ok(())
    }
}


fn clamp_magnitude(mut v: f64, max: f64) -> f64 {
    let max_p = max.abs();
    let max_n = -max_p;
    if v > max_p {
        v = max_p;
    }
    if v < max_n {
        v = max_n;
    }

    v
}
