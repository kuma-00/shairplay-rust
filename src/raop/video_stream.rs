//! Video stream receiver for AirPlay 2 screen mirroring (stream type 110).
//!
//! Accepts a TCP connection, reads 128-byte headers + variable-length payloads,
//! classifies packets, decrypts Payload types, and delivers to VideoSession.

use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::BytesMut;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, info, trace, warn};

use crate::crypto::video_cipher::VideoCipher;
use crate::raop::video::{PacketKind, VideoHandler, VideoPacket, VideoRestartHandle, VideoSession};

const VIDEO_HEADER_LEN: usize = 128;
const MAX_VIDEO_PAYLOAD_LEN: usize = 32 * 1024 * 1024;
/// Drop a video connection whose peer stalls mid-read, freeing the task and port.
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Run the video stream receiver. Accepts one TCP connection and processes packets.
pub(crate) async fn run(
    listener: TcpListener,
    cipher: VideoCipher,
    handler: Arc<dyn VideoHandler>,
    restart: VideoRestartHandle,
) {
    let mut cipher = cipher;
    loop {
        if restart.take_request() {
            info!("Video stream restart requested before accept");
        }
        let (stream, addr) = match listener.accept().await {
            Ok(s) => s,
            Err(e) => {
                warn!("Video stream accept failed: {e}");
                return;
            }
        };
        info!(%addr, "Video stream client connected");
        restart.mark_session_started();
        let session = handler.video_init();
        process(stream, &mut cipher, session, &restart).await;
    }
}

async fn process(
    mut stream: TcpStream,
    cipher: &mut VideoCipher,
    mut session: Box<dyn VideoSession>,
    restart: &VideoRestartHandle,
) {
    let mut header = [0u8; VIDEO_HEADER_LEN];

    loop {
        if restart.is_requested() {
            info!("Video stream client disconnected for restart");
            break;
        }
        // Read 128-byte header
        match tokio::time::timeout(READ_TIMEOUT, stream.read_exact(&mut header)).await {
            Ok(Ok(_)) => {}
            Ok(Err(_)) => {
                debug!("Video stream ended");
                break;
            }
            Err(_) => {
                debug!("Video stream header read timed out");
                break;
            }
        }
        let header_received_at = Instant::now();

        // Parse header fields (little-endian)
        let payload_len = u32::from_le_bytes([header[0], header[1], header[2], header[3]]) as usize;
        let packet_type = u16::from_le_bytes([header[4], header[5]]);
        let timestamp = u64::from_le_bytes([
            header[8], header[9], header[10], header[11], header[12], header[13], header[14], header[15],
        ]);

        if payload_len == 0 {
            continue;
        }
        if payload_len > MAX_VIDEO_PAYLOAD_LEN {
            warn!(payload_len, "Video payload exceeds maximum allowed size");
            break;
        }

        // Read payload
        let mut payload = BytesMut::zeroed(payload_len);
        match tokio::time::timeout(READ_TIMEOUT, stream.read_exact(&mut payload)).await {
            Ok(Ok(_)) => {}
            Ok(Err(_)) => {
                debug!("Video stream ended during payload read");
                break;
            }
            Err(_) => {
                debug!("Video stream payload read timed out");
                break;
            }
        }
        let payload_received_at = Instant::now();

        // Classify packet
        let kind = match packet_type {
            1 => {
                if payload.windows(4).any(|tag| matches!(tag, b"hvc1" | b"hev1" | b"hvcC")) {
                    PacketKind::HvcC
                } else {
                    PacketKind::AvcC
                }
            }
            0 | 4096 => PacketKind::Payload,
            5 => PacketKind::Plist,
            other => PacketKind::Other(other),
        };

        // Decrypt payload packets
        if matches!(kind, PacketKind::Payload) {
            cipher.decrypt(&mut payload);
        }
        let decrypted_at = Instant::now();

        trace!(?kind, timestamp, payload_len, "Video packet");
        session.on_video(VideoPacket {
            kind,
            timestamp,
            payload: payload.freeze(),
            header_received_at,
            payload_received_at,
            decrypted_at,
        });
    }
    session.on_video_end();
}
