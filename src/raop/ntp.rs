//! NTP timing responder for AirPlay legacy connections.
//!
//! Sends timing requests to the iPhone and responds to incoming
//! timing requests. Required for legacy (non-PTP) AirPlay connections.

/// Seconds between the NTP epoch (1900-01-01) and the UNIX epoch (1970-01-01).
const NTP_UNIX_EPOCH_OFFSET_SECS: u64 = 0x83AA_7E80; // 2_208_988_800

fn ntp_system_time() -> u64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = (now.as_secs() + NTP_UNIX_EPOCH_OFFSET_SECS) as u32;
    let frac = (((now.subsec_nanos() as u64) << 32) / 1_000_000_000) as u32;
    (u64::from(secs) << 32) | u64::from(frac)
}

fn read_ntp(buf: &[u8], off: usize) -> Option<u64> {
    let bytes = buf.get(off..off + 8)?.try_into().ok()?;
    Some(u64::from_be_bytes(bytes))
}

fn put_ntp(buf: &mut [u8], off: usize, value: u64) {
    buf[off..off + 8].copy_from_slice(&value.to_be_bytes());
}

/// Calculate the NTP clock offset `remote - local` from a complete four-timestamp exchange.
/// Wrapping subtraction also handles the NTP era boundary as long as the clocks differ by <68 years.
fn clock_offset(t1: u64, t2: u64, t3: u64, t4: u64) -> i64 {
    let remote_receive_minus_local_send = t2.wrapping_sub(t1) as i64;
    let remote_send_minus_local_receive = t3.wrapping_sub(t4) as i64;
    ((i128::from(remote_receive_minus_local_send) + i128::from(remote_send_minus_local_receive)) / 2) as i64
}

fn apply_offset(local: u64, offset: i64) -> u64 {
    local.wrapping_add(offset as u64)
}

/// timing requests and sends periodic keepalives. Required for legacy AirPlay
/// connections where the iPhone expects NTP sync before streaming audio.
pub(crate) fn spawn_ntp_responder(tsock: tokio::net::UdpSocket, remote_timing: std::net::SocketAddr) {
    tokio::spawn(async move {
        let mut buf = [0u8; 128];
        let mut remote_clock_offset = 0i64;

        // Send initial timing requests to iPhone
        if remote_timing.port() > 0 {
            tracing::debug!(%remote_timing, "NTP: sending initial timing requests");
            for _ in 0..3 {
                let mut req = [0u8; 32];
                req[0] = 0x80;
                req[1] = 0xd2;
                req[2] = 0x00;
                req[3] = 0x07;
                put_ntp(&mut req, 24, apply_offset(ntp_system_time(), remote_clock_offset));
                let _ = tsock.send_to(&req, remote_timing).await;
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }

        loop {
            let timeout = tokio::time::sleep(std::time::Duration::from_secs(3));
            tokio::select! {
                result = tsock.recv_from(&mut buf) => {
                    match result {
                        Ok((len, addr)) if len >= 32 && buf[1] & 0x7f == 0x52 => {
                            // Timing request — send response
                            let mut resp = [0u8; 32];
                            resp[..32].copy_from_slice(&buf[..32]);
                            resp[1] = 0xd3;
                            resp[8..16].copy_from_slice(&buf[24..32]);
                            let now = apply_offset(ntp_system_time(), remote_clock_offset);
                            put_ntp(&mut resp, 16, now);
                            put_ntp(&mut resp, 24, now);
                            let _ = tsock.send_to(&resp, addr).await;
                        }
                        Ok((len, _)) if len >= 32 && buf[1] & 0x7f == 0x53 => {
                            if let (Some(t1), Some(t2), Some(t3)) =
                                (read_ntp(&buf, 8), read_ntp(&buf, 16), read_ntp(&buf, 24))
                            {
                                let t4 = ntp_system_time();
                                remote_clock_offset = clock_offset(t1, t2, t3, t4);
                                tracing::debug!(
                                    offset_ms = remote_clock_offset as f64 * 1_000.0 / 4_294_967_296.0,
                                    "NTP: sender clock synchronized"
                                );
                            }
                        }
                        Ok(_) => {}
                        Err(_) => break,
                    }
                }
                _ = timeout => {
                    if remote_timing.port() > 0 {
                        let mut req = [0u8; 32];
                        req[0] = 0x80;
                        req[1] = 0xd2;
                        req[2] = 0x00;
                        req[3] = 0x07;
                        put_ntp(&mut req, 24, apply_offset(ntp_system_time(), remote_clock_offset));
                        let _ = tsock.send_to(&req, remote_timing).await;
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_remote_clock_offset_and_applies_it() {
        let second = 1u64 << 32;
        let t1 = 100 * second;
        let t2 = 91 * second + second / 10;
        let t3 = 91 * second + second / 5;
        let t4 = 100 * second + 3 * second / 10;
        let offset = clock_offset(t1, t2, t3, t4);
        assert_eq!(offset, -(9 * second as i64));
        assert_eq!(apply_offset(t4, offset), 91 * second + 3 * second / 10);
    }

    #[test]
    fn ntp_field_round_trips() {
        let mut packet = [0u8; 32];
        put_ntp(&mut packet, 24, 0x83b2_46ed_1234_5678);
        assert_eq!(read_ntp(&packet, 24), Some(0x83b2_46ed_1234_5678));
    }

    #[test]
    fn offset_average_does_not_overflow_for_wall_clock_to_uptime_clock() {
        let local = 0xedfc_f2af_0000_0000;
        let remote = 0x83b2_46ed_0000_0000;
        let offset = clock_offset(local, remote, remote, local);
        assert_eq!(apply_offset(local, offset), remote);
    }
}
