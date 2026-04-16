use std::time::Duration;

use common::errors::*;
use common::array_mut_ref;
use net::udp::*;
use net::ip::SocketAddr;
use executor::sync::AsyncMutex;
use executor::lock_async;
use sys::ControlMessage;

const NANOS_PER_SECOND: u64 = 1_000_000_000;

/// PTP v2 sync packet.
/// Carefully crafted for Raspberry Pi compatibility.
const PACKET_TEMPLATE: [u8; 8] = {
    let mut payload = [0u8; 8];
    payload[0] = 0x00; // MsgType: Sync (0)
    payload[1] = 0x02; // Version: 2
    payload[2] = 0x00; // Length High
    payload[3] = 0x2c; // Length Low (44 bytes)
    payload[4] = 0x00; // Domain 0
    payload[5] = 0x00; // Reserved

    // Flags (Two-step)
    payload[6] = 0x02; 
    payload[7] = 0x00; 

    /*
    // 8..16 => correction field (used by transparent clocks)
    // 16..20 => reserved
    // 20..30 => source port id

    // Sequence ID
    payload[30] = 0x00;
    payload[31] = 0x01;

    payload[32] = 0; // Control field.
    payload[33] = 0; // log message interval.

    // 34..44 : origin timestamp
    */

    payload
};


/// Wrapper around a UDP socket which sends/receives hardware timestamped packets.
///
/// Currently this always sends/receives packets which appear to be PTP v2 packets
/// since certain ethernet peripherals (e.g. Raspberry Pi) will only timestamp
/// packets that are in this specific form.
pub struct TimestampedUdpSocket {
    sock: UdpSocket,
    sender_lock: AsyncMutex<()>
}

impl TimestampedUdpSocket {
    pub async fn create(bind_addr: SocketAddr, iface_name: &str) -> Result<Self> {
        let mut sock_opts = UdpBindOptions::new();

        sock_opts
        .bind_to_device(iface_name)
        .enable_hardware_timestamping();

        let sock = net::udp::UdpSocket::bind_with_options(
            bind_addr,
            &sock_opts
        ).await?;

        Ok(Self { sock, sender_lock: AsyncMutex::new(()) })
    }

    pub async fn send_to(&self, data: &[u8], addr: &SocketAddr) -> Result<u64> {
        let mut buf = [0u8; 256];
        buf[0..PACKET_TEMPLATE.len()].copy_from_slice(&PACKET_TEMPLATE);
        if data.len() > buf.len() - PACKET_TEMPLATE.len() {
            return Err(err_msg("Too much data to send"));
        }

        let n = PACKET_TEMPLATE.len() + data.len();
        buf[PACKET_TEMPLATE.len()..n].copy_from_slice(data);

        *array_mut_ref![buf, 2, 2] = (n as u16).to_be_bytes();

        // TODO: If any part of this fails, they we may get out of sync with the error queue.
        lock_async!(lock <= self.sender_lock.lock().await?, {

            let n_sent = self.sock.send_to(&buf[0..n], addr).await?;
            if n_sent != n {
                return Err(err_msg("Wrong number of bytes sent"));
            }

            // TODO: Replace with a simpler fixed size buffer.
            let mut control_msgs = [
                sys::ControlMessage::ScmTimestamping(sys::bindings::scm_timestamping64::default()),
                sys::ControlMessage::ScmTimestamping(sys::bindings::scm_timestamping64::default()),
                sys::ControlMessage::ScmTimestamping(sys::bindings::scm_timestamping64::default()),
                sys::ControlMessage::ScmTimestamping(sys::bindings::scm_timestamping64::default()),
            ];

            // Usually if the timeout fails, they we sent some packet that isn't being properly
            // noticed by the ethernet hardware so never got timestamped.
            let (_, num_msgs, _) = executor::timeout(
                Duration::from_secs(1),
                self.sock.recv_error(&mut [], &mut control_msgs)   
            )
            .await
            .map_err(|e| err_msg("Timed out while waiting for TX timeout"))??;

            self.get_timestamp(&control_msgs[0..num_msgs])
        })
    }

    pub async fn recv_from(&self, data: &mut [u8]) -> Result<(usize, u64, SocketAddr)> {
        let mut buf = [0u8; 256];

        // TODO: Replace with a simpler fixed size buffer.
        let mut control_msgs = [
            sys::ControlMessage::ScmTimestamping(sys::bindings::scm_timestamping64::default()),
            sys::ControlMessage::ScmTimestamping(sys::bindings::scm_timestamping64::default()),
            sys::ControlMessage::ScmTimestamping(sys::bindings::scm_timestamping64::default()),
            sys::ControlMessage::ScmTimestamping(sys::bindings::scm_timestamping64::default()),
        ];

        let (num_bytes, num_msgs, addr) = self.sock.recv_msg(&mut buf, &mut control_msgs).await?;
        if num_bytes < PACKET_TEMPLATE.len() {
            return Err(err_msg("Received message too short."));
        }

        if num_bytes == buf.len() {
            return Err(err_msg("Possible size overflow"));
        }


        let mut expected_header = PACKET_TEMPLATE.clone();
        *array_mut_ref![expected_header, 2, 2] = (num_bytes as u16).to_be_bytes();

        // TODO: Need to adjust the length field.
        if &buf[0..PACKET_TEMPLATE.len()] != &expected_header {
            return Err(err_msg("Beginning of the received message does not follow the PTP template"))
        }

        let timestamp = self.get_timestamp(&control_msgs[0..num_msgs])?;
        
        let n = num_bytes - PACKET_TEMPLATE.len();
        if n > data.len() {
            return Err(err_msg("Not enough output buffer space for entire message"));
        }

        data[0..n].copy_from_slice(&buf[PACKET_TEMPLATE.len()..num_bytes]);

        // TODO: Normalize sys::SockAddr into net::ip one in the recv_msg call.
        Ok((n, timestamp, addr.into()))
    }

    fn get_timestamp(&self, msgs: &[ControlMessage]) -> Result<u64> {
        let mut timestamp = None;

        for msg in msgs {
            if let sys::ControlMessage::ScmTimestamping(v) = msg {
                // TODO: Check other fields to verify ts[2] is the hardware timestamp.
                // TODO: think about whether this will ever be negative.
                let ts = &v.ts[2];
                timestamp = Some((ts.tv_nsec as u64) + (ts.tv_sec as u64) * NANOS_PER_SECOND);
                break;
            }
        }

        let timestamp = timestamp
            .ok_or_else(|| err_msg("No timestamp generated for received packet"))?;

        Ok(timestamp)
    }
}
