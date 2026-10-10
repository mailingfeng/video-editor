use crate::contracts::{AppError, ErrorCode};
use std::{sync::Arc, time::Duration};
use tokio::{sync::Mutex, time::Instant};

pub const EXPIRES_AT_MS: u64 = 1_798_732_800_000;
const NTP_UNIX_OFFSET: u64 = 2_208_988_800;
const REFRESH_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseStatus {
    pub expires_at_ms: u64,
    pub effective_time_ms: u64,
    pub expired: bool,
    pub ntp_available: bool,
}

#[async_trait::async_trait]
trait TimeSource: Send + Sync {
    fn local_time_ms(&self) -> u64;
    async fn network_time_ms(&self) -> Option<u64>;
}

#[derive(Default)]
struct ClockState {
    anchor: Option<(u64, Instant)>,
    checked_at: Option<Instant>,
    ntp_available: bool,
}

pub struct LicenseService {
    source: Arc<dyn TimeSource>,
    clock: Mutex<ClockState>,
}
impl LicenseService {
    pub fn new() -> Self {
        Self {
            source: Arc::new(SystemTimeSource),
            clock: Mutex::new(ClockState::default()),
        }
    }
    pub async fn status(&self) -> LicenseStatus {
        // Serialize checks so simultaneous commands cannot overwrite a later
        // observed time with an older response, or flood the NTP servers.
        let mut clock = self.clock.lock().await;
        let sampled_at = Instant::now();
        let local_sample = self.source.local_time_ms();
        let mut effective = local_sample;
        if clock
            .checked_at
            .is_none_or(|checked| checked.elapsed() >= REFRESH_INTERVAL)
        {
            let network = self.source.network_time_ms().await;
            clock.ntp_available = network.is_some();
            if let Some(time) = network {
                effective = effective.max(time);
            }
            clock.checked_at = Some(Instant::now());
        }
        effective = effective.max(self.source.local_time_ms());
        effective =
            effective.max(local_sample.saturating_add(sampled_at.elapsed().as_millis() as u64));
        let advanced_anchor = clock
            .anchor
            .map(|(time, observed)| time.saturating_add(observed.elapsed().as_millis() as u64));
        if let Some(time) = advanced_anchor {
            effective = effective.max(time);
        }
        // Keep the original monotonic anchor when it wins, retaining fractional
        // milliseconds across rapid cached queries rather than rounding each one.
        if advanced_anchor.is_none_or(|time| effective > time) {
            clock.anchor = Some((effective, Instant::now()));
        }
        LicenseStatus {
            expires_at_ms: EXPIRES_AT_MS,
            effective_time_ms: effective,
            expired: effective >= EXPIRES_AT_MS,
            ntp_available: clock.ntp_available,
        }
    }
    pub async fn require_active(&self) -> Result<(), AppError> {
        if self.status().await.expired {
            Err(AppError::new(
                ErrorCode::LicenseExpired,
                "使用许可已于 2026-12-31 到期，暂时无法开始处理。请联系软件提供方更新许可。",
            ))
        } else {
            Ok(())
        }
    }
}
impl Default for LicenseService {
    fn default() -> Self {
        Self::new()
    }
}

struct SystemTimeSource;
#[async_trait::async_trait]
impl TimeSource for SystemTimeSource {
    fn local_time_ms(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
    async fn network_time_ms(&self) -> Option<u64> {
        let limit = Duration::from_secs(2);
        let (aliyun, ntsc) = tokio::join!(
            query_ntp("ntp1.aliyun.com:123", limit),
            query_ntp("ntp1.ntsc.ac.cn:123", limit),
        );
        [aliyun, ntsc]
            .into_iter()
            .flatten()
            .map(|(time, received)| time.saturating_add(received.elapsed().as_millis() as u64))
            .max()
    }
}

// RFC 5905: require a synchronized server response, stratum 1..15,
// the echoed originate cookie and a nonzero transmit timestamp.
fn parse_reply(packet: &[u8], cookie: [u8; 8]) -> Option<u64> {
    if packet.len() < 48
        || packet[0] >> 6 == 3
        || ![3, 4].contains(&((packet[0] >> 3) & 7))
        || packet[0] & 7 != 4
        || !(1..=15).contains(&packet[1])
        || packet[24..32] != cookie
        || packet[40..48] == [0; 8]
    {
        return None;
    }
    let seconds = u32::from_be_bytes(packet[40..44].try_into().ok()?) as u64;
    let fraction = u32::from_be_bytes(packet[44..48].try_into().ok()?) as u64;
    // Era 0 through 2036; small seconds are era 1 after the 32-bit wrap.
    let seconds = if seconds < NTP_UNIX_OFFSET {
        seconds + (1u64 << 32)
    } else {
        seconds
    };
    Some((seconds - NTP_UNIX_OFFSET) * 1000 + ((fraction * 1000) >> 32))
}

async fn query_ntp(address: &str, limit: Duration) -> Option<(u64, Instant)> {
    tokio::time::timeout(limit, async {
        let server = tokio::net::lookup_host(address).await.ok()?.next()?;
        let bind = if server.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        };
        let socket = tokio::net::UdpSocket::bind(bind).await.ok()?;
        socket.connect(server).await.ok()?;
        let cookie: [u8; 8] = uuid::Uuid::new_v4().as_bytes()[..8].try_into().ok()?;
        let mut request = [0u8; 48];
        request[0] = 0x23;
        request[40..48].copy_from_slice(&cookie);
        let sent = Instant::now();
        socket.send(&request).await.ok()?;
        let mut packet = [0u8; 512];
        let length = socket.recv(&mut packet).await.ok()?;
        let received = Instant::now();
        let time = parse_reply(&packet[..length], cookie)?;
        Some((
            time.saturating_add(sent.elapsed().as_millis() as u64 / 2),
            received,
        ))
    })
    .await
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests;
