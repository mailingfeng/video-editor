use super::*;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

struct FakeTime {
    local: AtomicU64,
    network: Mutex<Option<u64>>,
    calls: AtomicUsize,
}
#[async_trait::async_trait]
impl TimeSource for FakeTime {
    fn local_time_ms(&self) -> u64 {
        self.local.load(Ordering::Relaxed)
    }
    async fn network_time_ms(&self) -> Option<u64> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        *self.network.lock().await
    }
}
fn fixture(local: u64, network: Option<u64>) -> (LicenseService, Arc<FakeTime>) {
    let source = Arc::new(FakeTime {
        local: AtomicU64::new(local),
        network: Mutex::new(network),
        calls: AtomicUsize::new(0),
    });
    (
        LicenseService {
            source: source.clone(),
            clock: Mutex::new(ClockState::default()),
        },
        source,
    )
}

#[tokio::test(start_paused = true)]
async fn advances_the_initial_local_sample_during_a_slow_network_check() {
    struct SlowTime;
    #[async_trait::async_trait]
    impl TimeSource for SlowTime {
        fn local_time_ms(&self) -> u64 {
            EXPIRES_AT_MS - 1000
        }
        async fn network_time_ms(&self) -> Option<u64> {
            tokio::time::sleep(Duration::from_secs(2)).await;
            None
        }
    }
    let license = LicenseService {
        source: Arc::new(SlowTime),
        clock: Mutex::new(ClockState::default()),
    };
    assert!(license.status().await.expired);
}

#[tokio::test(start_paused = true)]
async fn allows_the_last_millisecond_of_december_31_in_china() {
    let (license, _) = fixture(EXPIRES_AT_MS - 1, None);
    assert!(!license.status().await.expired);
    assert!(license.require_active().await.is_ok());
    tokio::time::advance(Duration::from_millis(1)).await;
    assert_eq!(
        license.require_active().await.unwrap_err().code,
        ErrorCode::LicenseExpired
    );
}

#[tokio::test(start_paused = true)]
async fn rejects_at_the_exact_deadline_with_a_friendly_error() {
    let (license, _) = fixture(EXPIRES_AT_MS, None);
    let error = license.require_active().await.unwrap_err();
    assert_eq!(error.code, ErrorCode::LicenseExpired);
    assert!(error.message.contains("2026-12-31"));
    assert!(error.message.contains("更新许可"));
}

#[tokio::test(start_paused = true)]
async fn chooses_later_local_or_network_time() {
    for (local, remote) in [
        (EXPIRES_AT_MS, EXPIRES_AT_MS - 5000),
        (EXPIRES_AT_MS - 5000, EXPIRES_AT_MS),
    ] {
        let (license, _) = fixture(local, Some(remote));
        let status = license.status().await;
        assert_eq!(status.effective_time_ms, EXPIRES_AT_MS);
        assert!(status.expired);
        assert!(status.ntp_available);
    }
}

#[tokio::test(start_paused = true)]
async fn offline_falls_back_to_local_time() {
    let (license, _) = fixture(EXPIRES_AT_MS - 100_000, None);
    let status = license.status().await;
    assert!(!status.expired);
    assert!(!status.ntp_available);
    assert_eq!(status.effective_time_ms, EXPIRES_AT_MS - 100_000);
}

#[tokio::test(start_paused = true)]
async fn clock_rollback_and_failed_refresh_cannot_unexpire_the_current_session() {
    let (license, source) = fixture(EXPIRES_AT_MS - 500, Some(EXPIRES_AT_MS - 100));
    assert!(!license.status().await.expired);
    source
        .local
        .store(EXPIRES_AT_MS - 1_000_000, Ordering::Relaxed);
    tokio::time::advance(Duration::from_millis(100)).await;
    assert!(license.status().await.expired);
    *source.network.lock().await = None;
    tokio::time::advance(REFRESH_INTERVAL).await;
    let status = license.status().await;
    assert!(status.expired);
    assert!(!status.ntp_available);
}

#[tokio::test(start_paused = true)]
async fn caches_network_checks_and_refreshes_after_one_minute() {
    let (license, source) = fixture(EXPIRES_AT_MS - 100_000, None);
    license.status().await;
    license.require_active().await.unwrap();
    assert_eq!(source.calls.load(Ordering::Relaxed), 1);
    *source.network.lock().await = Some(EXPIRES_AT_MS);
    tokio::time::advance(REFRESH_INTERVAL).await;
    assert!(license.status().await.expired);
    assert_eq!(source.calls.load(Ordering::Relaxed), 2);
}

fn reply(cookie: [u8; 8], time_ms: u64) -> [u8; 48] {
    let mut packet = [0; 48];
    packet[0] = 0x24;
    packet[1] = 2;
    packet[24..32].copy_from_slice(&cookie);
    let seconds = (time_ms / 1000 + NTP_UNIX_OFFSET) as u32;
    let fraction = (((time_ms % 1000) << 32) / 1000) as u32;
    packet[40..44].copy_from_slice(&seconds.to_be_bytes());
    packet[44..48].copy_from_slice(&fraction.to_be_bytes());
    packet
}

#[test]
fn validates_ntp_header_cookie_and_timestamp_including_era_wrap() {
    let cookie = [17; 8];
    let packet = reply(cookie, EXPIRES_AT_MS);
    assert_eq!(parse_reply(&packet, cookie), Some(EXPIRES_AT_MS));
    let wrapped = 2_100_000_000_000;
    assert_eq!(parse_reply(&reply(cookie, wrapped), cookie), Some(wrapped));
    assert!(parse_reply(&packet[..47], cookie).is_none());
    assert!(parse_reply(&packet, [18; 8]).is_none());
    for (index, value) in [(0, 0xe4), (0, 0x23), (0, 0x14), (1, 0), (1, 16)] {
        let mut invalid = packet;
        invalid[index] = value;
        assert!(
            parse_reply(&invalid, cookie).is_none(),
            "invalid field {index}={value}"
        );
    }
    let mut zero = packet;
    zero[40..48].fill(0);
    assert!(parse_reply(&zero, cookie).is_none());
}

#[tokio::test]
async fn queries_a_real_udp_socket_and_matches_the_request_cookie() {
    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let address = socket.local_addr().unwrap().to_string();
    let server = tokio::spawn(async move {
        let mut request = [0; 48];
        let (length, client) = socket.recv_from(&mut request).await.unwrap();
        assert_eq!(length, 48);
        assert_eq!(request[0], 0x23);
        socket
            .send_to(
                &reply(request[40..48].try_into().unwrap(), EXPIRES_AT_MS),
                client,
            )
            .await
            .unwrap();
    });
    let (time, _) = query_ntp(&address, Duration::from_secs(1)).await.unwrap();
    assert!((EXPIRES_AT_MS..EXPIRES_AT_MS + 500).contains(&time));
    server.await.unwrap();
}

#[tokio::test]
async fn udp_timeout_is_bounded_and_returns_no_time() {
    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let start = Instant::now();
    assert!(query_ntp(
        &socket.local_addr().unwrap().to_string(),
        Duration::from_millis(40)
    )
    .await
    .is_none());
    assert!(start.elapsed() < Duration::from_secs(1));
}
