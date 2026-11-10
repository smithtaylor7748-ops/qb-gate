//! 原生 UDP 出口采样。按 RFC 5389 独立实现，不依赖浏览器、HTTP 代理或账户凭据。
//!
//! 只有使用者显式开始这项检测才调用 [`probe`]。这里只记录 STUN 的观测事实；
//! 与同地址族 HTTP 出口的比较由上层负责，不能据此认证浏览器行为或宣称没有泄漏。
//! 每个服务分别采样 IPv4 / IPv6，缺少地址、超时和协议错误都保留，不当成通过。

use serde::Serialize;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::task::JoinSet;
use tokio::time::{timeout_at, Instant};
use ts_rs::TS;

const COOKIE: u32 = 0x2112_a442;
const HEADER_LEN: usize = 20;
const MAX_RESPONSE: usize = 4096;
const MAX_ADDRESSES_PER_FAMILY: usize = 2;

/// 不同运营方的公开 STUN 服务；端口是服务协议配置，不是本机监听端口。
pub const DEFAULT_SERVERS: &[(&str, u16)] =
    &[("stun.cloudflare.com", 3478), ("stun.l.google.com", 19302)];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum StunFamily {
    Ipv4,
    Ipv6,
}

impl StunFamily {
    fn matches(self, address: SocketAddr) -> bool {
        matches!(
            (self, address),
            (Self::Ipv4, SocketAddr::V4(_)) | (Self::Ipv6, SocketAddr::V6(_))
        )
    }

    fn bind_address(self) -> SocketAddr {
        match self {
            Self::Ipv4 => SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)),
            Self::Ipv6 => SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum StunStatus {
    Completed,
    Timeout,
    Unreachable,
    ProtocolError,
    NoAddress,
}

/// 采样覆盖度，不是安全结论。Complete 也需要上层按地址族比较 HTTP 出口。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum StunCoverage {
    Unknown,
    Partial,
    Complete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct StunObservation {
    pub server: String,
    pub family: StunFamily,
    pub status: StunStatus,
    /// 实际联系的服务端 socket 地址；DNS 未完成时为空。
    pub server_address: Option<String>,
    pub mapped_address: Option<String>,
    pub mapped_port: Option<u16>,
    pub elapsed_ms: u32,
    /// 仅用预定义说明，不包含操作系统错误原文或用户配置。
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct StunReport {
    pub checked_at: String,
    pub coverage: StunCoverage,
    pub observations: Vec<StunObservation>,
}

#[derive(Clone, Copy)]
struct Limits {
    dns: Duration,
    datagram: Duration,
    total: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            dns: Duration::from_secs(3),
            datagram: Duration::from_secs(2),
            total: Duration::from_secs(8),
        }
    }
}

fn elapsed(start: Instant) -> u32 {
    start.elapsed().as_millis().min(u128::from(u32::MAX)) as u32
}

fn observation(
    server: &str,
    family: StunFamily,
    status: StunStatus,
    detail: &str,
) -> StunObservation {
    StunObservation {
        server: server.to_owned(),
        family,
        status,
        server_address: None,
        mapped_address: None,
        mapped_port: None,
        elapsed_ms: 0,
        detail: Some(detail.to_owned()),
    }
}

fn coverage(observations: &[StunObservation]) -> StunCoverage {
    let completed = observations
        .iter()
        .filter(|item| item.status == StunStatus::Completed)
        .count();
    if completed == 0 {
        StunCoverage::Unknown
    } else if completed == observations.len() {
        StunCoverage::Complete
    } else {
        StunCoverage::Partial
    }
}

/// 一次有界的显式检测：两个服务并发，每个服务的两个地址族也并发。
///
/// UDP 直接使用系统路由，HTTP_PROXY / HTTPS_PROXY 不参与；TUN 仍可能接管它。
/// 不写历史、不缓存、不重配网络。取消本 future 会销毁 JoinSet 并中止子任务。
pub async fn probe() -> StunReport {
    probe_servers(DEFAULT_SERVERS, Limits::default()).await
}

async fn probe_servers(servers: &[(&str, u16)], limits: Limits) -> StunReport {
    let started = Instant::now();
    let deadline = started + limits.total;
    let mut observations = Vec::with_capacity(servers.len() * 2);
    let mut tasks = JoinSet::new();
    for (index, &(host, port)) in servers.iter().enumerate() {
        let label = format!("{host}:{port}");
        for family in [StunFamily::Ipv4, StunFamily::Ipv6] {
            observations.push(observation(
                &label,
                family,
                StunStatus::Timeout,
                "达到整次检测时限",
            ));
        }
        let host = host.to_owned();
        tasks.spawn(async move { (index, probe_server(&host, port, limits, deadline).await) });
    }
    while !tasks.is_empty() {
        match timeout_at(deadline, tasks.join_next()).await {
            Ok(Some(Ok((index, results)))) => {
                for (offset, result) in results.into_iter().enumerate() {
                    observations[index * 2 + offset] = result;
                }
            }
            Ok(Some(Err(_))) => {
                // 丢失的任务保留未完成项，不把已有成功结果拼成全绿。
            }
            Ok(None) | Err(_) => break,
        }
    }
    tasks.shutdown().await;
    for item in &mut observations {
        if item.elapsed_ms == 0 && item.status == StunStatus::Timeout {
            item.elapsed_ms = elapsed(started);
        }
    }
    StunReport {
        checked_at: chrono::Utc::now().to_rfc3339(),
        coverage: coverage(&observations),
        observations,
    }
}

async fn probe_server(
    host: &str,
    port: u16,
    limits: Limits,
    deadline: Instant,
) -> [StunObservation; 2] {
    let started = Instant::now();
    let label = format!("{host}:{port}");
    let addresses = if let Ok(ip) = host.parse::<IpAddr>() {
        Ok(vec![SocketAddr::new(ip, port)])
    } else {
        match timeout_at(
            deadline.min(started + limits.dns),
            tokio::net::lookup_host((host, port)),
        )
        .await
        {
            Ok(Ok(addresses)) => Ok(addresses.take(32).collect::<Vec<_>>()),
            Ok(Err(_)) => Err((StunStatus::Unreachable, "STUN 服务域名解析失败")),
            Err(_) => Err((StunStatus::Timeout, "STUN 服务域名解析超时")),
        }
    };
    match addresses {
        Ok(addresses) => {
            let (v4, v6) = tokio::join!(
                probe_family(
                    &label,
                    StunFamily::Ipv4,
                    &addresses,
                    limits,
                    deadline,
                    started
                ),
                probe_family(
                    &label,
                    StunFamily::Ipv6,
                    &addresses,
                    limits,
                    deadline,
                    started
                ),
            );
            [v4, v6]
        }
        Err((status, detail)) => [StunFamily::Ipv4, StunFamily::Ipv6].map(|family| {
            let mut item = observation(&label, family, status, detail);
            item.elapsed_ms = elapsed(started);
            item
        }),
    }
}

async fn probe_family(
    server: &str,
    family: StunFamily,
    addresses: &[SocketAddr],
    limits: Limits,
    deadline: Instant,
    started: Instant,
) -> StunObservation {
    let mut item = observation(
        server,
        family,
        StunStatus::NoAddress,
        "该服务没有返回此地址族的解析记录",
    );
    let mut attempted = Vec::new();
    for &peer in addresses.iter().filter(|address| family.matches(**address)) {
        if attempted.contains(&peer) {
            continue;
        }
        if attempted.len() >= MAX_ADDRESSES_PER_FAMILY {
            break;
        }
        if Instant::now() >= deadline {
            item.status = StunStatus::Timeout;
            item.detail = Some("达到整次检测时限".to_owned());
            break;
        }
        attempted.push(peer);
        item.server_address = Some(peer.to_string());
        let attempt_deadline = deadline.min(Instant::now() + limits.datagram);
        match timeout_at(attempt_deadline, exchange(peer, family)).await {
            Ok(Ok(mapped)) => {
                item.status = StunStatus::Completed;
                item.mapped_address = Some(mapped.ip().to_string());
                item.mapped_port = Some(mapped.port());
                item.detail = None;
                break;
            }
            Ok(Err((status, detail))) => {
                item.status = status;
                item.detail = Some(detail.to_owned());
            }
            Err(_) => {
                item.status = StunStatus::Timeout;
                item.detail = Some("STUN UDP 请求超时；无法据此确认出口".to_owned());
            }
        }
    }
    item.elapsed_ms = elapsed(started);
    item
}

/// 使用 UUID v4 中两个无版本/variant 位的片段，保留完整 96 位随机事务 ID。
fn transaction_id() -> [u8; 12] {
    let random = uuid::Uuid::new_v4();
    let mut transaction = [0; 12];
    transaction[..6].copy_from_slice(&random.as_bytes()[..6]);
    transaction[6..].copy_from_slice(&random.as_bytes()[10..]);
    transaction
}

fn request(transaction: &[u8; 12]) -> [u8; HEADER_LEN] {
    let mut packet = [0; HEADER_LEN];
    packet[1] = 1; // Binding Request，无属性、无凭据。
    packet[4..8].copy_from_slice(&COOKIE.to_be_bytes());
    packet[8..].copy_from_slice(transaction);
    packet
}

async fn exchange(
    peer: SocketAddr,
    family: StunFamily,
) -> Result<SocketAddr, (StunStatus, &'static str)> {
    let socket = UdpSocket::bind(family.bind_address())
        .await
        .map_err(|_| (StunStatus::Unreachable, "无法创建此地址族的 UDP socket"))?;
    socket
        .connect(peer)
        .await
        .map_err(|_| (StunStatus::Unreachable, "无法建立到 STUN 服务的 UDP 路径"))?;
    let transaction = transaction_id();
    socket
        .send(&request(&transaction))
        .await
        .map_err(|_| (StunStatus::Unreachable, "无法发送 STUN 请求"))?;
    // 多留一字节以识别超长数据报；绝不解析截断后的前缀为成功响应。
    let mut bytes = [0; MAX_RESPONSE + 1];
    let (len, source) = socket
        .recv_from(&mut bytes)
        .await
        .map_err(|_| (StunStatus::Unreachable, "STUN UDP 路径不可达"))?;
    if source != peer {
        return Err((StunStatus::ProtocolError, "STUN 响应来源与请求目标不符"));
    }
    if len > MAX_RESPONSE {
        return Err((StunStatus::ProtocolError, "STUN 响应超过大小上限"));
    }
    let mapped = parse_response(&bytes[..len], &transaction)
        .map_err(|detail| (StunStatus::ProtocolError, detail))?;
    if !family.matches(mapped) {
        return Err((StunStatus::ProtocolError, "STUN 映射地址与传输地址族不一致"));
    }
    Ok(mapped)
}

fn read_u16(bytes: &[u8]) -> u16 {
    u16::from_be_bytes([bytes[0], bytes[1]])
}

fn fingerprint(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0_u32.wrapping_sub(crc & 1)));
        }
    }
    !crc ^ 0x5354_554e
}

fn parse_response(bytes: &[u8], transaction: &[u8; 12]) -> Result<SocketAddr, &'static str> {
    if bytes.len() < HEADER_LEN || bytes.len() > MAX_RESPONSE {
        return Err("STUN 消息长度无效");
    }
    if bytes[4..8] != COOKIE.to_be_bytes() || bytes[8..20] != transaction[..] {
        return Err("STUN magic cookie 或事务 ID 不匹配");
    }
    let length = usize::from(read_u16(&bytes[2..4]));
    if length % 4 != 0 || HEADER_LEN + length != bytes.len() {
        return Err("STUN 消息声明长度与数据报不符");
    }
    if read_u16(&bytes[..2]) != 0x0101 {
        return Err("STUN 服务未返回 Binding Success Response");
    }
    let mut offset = HEADER_LEN;
    let mut mapped = None;
    while offset < bytes.len() {
        if offset + 4 > bytes.len() {
            return Err("STUN 属性头被截断");
        }
        let kind = read_u16(&bytes[offset..offset + 2]);
        let size = usize::from(read_u16(&bytes[offset + 2..offset + 4]));
        let end = offset + 4 + size;
        let padded_end = end + (4 - size % 4) % 4;
        if padded_end > bytes.len() {
            return Err("STUN 属性值或填充被截断");
        }
        let value = &bytes[offset + 4..end];
        match kind {
            0x0020 => {
                if mapped.is_some() {
                    return Err("STUN 包含重复映射地址属性");
                }
                mapped = Some(parse_address(value, transaction, true)?);
            }
            0x0001 => {
                // RFC 5389 兼容属性也检查结构；出口必须取 XOR-MAPPED-ADDRESS。
                parse_address(value, transaction, false)?;
            }
            0x0008 if size == 20 => {
                // 无凭据 Binding 不持有完整性密钥，不把此字段当作认证成功。
            }
            0x8028 => {
                if size != 4 || padded_end != bytes.len() {
                    return Err("STUN FINGERPRINT 属性长度或位置无效");
                }
                if value != fingerprint(&bytes[..offset]).to_be_bytes() {
                    return Err("STUN FINGERPRINT 校验失败");
                }
            }
            required if required < 0x8000 => return Err("STUN 含无法识别的必选属性"),
            _ => {} // 可选扩展只验证边界，按 RFC 忽略未知类型。
        }
        offset = padded_end;
    }
    mapped.ok_or("STUN 未返回 XOR-MAPPED-ADDRESS")
}

fn parse_address(
    value: &[u8],
    transaction: &[u8; 12],
    xor: bool,
) -> Result<SocketAddr, &'static str> {
    if value.len() < 4 || value[0] != 0 {
        return Err("STUN 映射地址属性头无效");
    }
    let port = read_u16(&value[2..4]) ^ if xor { (COOKIE >> 16) as u16 } else { 0 };
    if port == 0 {
        return Err("STUN 映射端口无效");
    }
    let mut mask = [0; 16];
    if xor {
        mask[..4].copy_from_slice(&COOKIE.to_be_bytes());
        mask[4..].copy_from_slice(transaction);
    }
    match (value[1], value.len()) {
        (1, 8) => {
            let address: [u8; 4] = std::array::from_fn(|i| value[4 + i] ^ mask[i]);
            Ok(SocketAddr::from((Ipv4Addr::from(address), port)))
        }
        (2, 20) => {
            let address: [u8; 16] = std::array::from_fn(|i| value[4 + i] ^ mask[i]);
            Ok(SocketAddr::from((Ipv6Addr::from(address), port)))
        }
        _ => Err("STUN 映射地址族或长度无效"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(transaction: &[u8; 12], mapped: SocketAddr) -> Vec<u8> {
        let mut value = vec![0, if mapped.is_ipv4() { 1 } else { 2 }];
        value.extend_from_slice(&(mapped.port() ^ 0x2112).to_be_bytes());
        let address = match mapped.ip() {
            IpAddr::V4(ip) => ip.octets().to_vec(),
            IpAddr::V6(ip) => ip.octets().to_vec(),
        };
        let mut mask = COOKIE.to_be_bytes().to_vec();
        mask.extend_from_slice(transaction);
        value.extend(address.iter().zip(&mask).map(|(byte, mask)| byte ^ mask));
        let mut packet = request(transaction).to_vec();
        packet[0] = 1;
        packet[2..4].copy_from_slice(&(4_u16 + value.len() as u16).to_be_bytes());
        packet.extend_from_slice(&[0, 0x20]);
        packet.extend_from_slice(&(value.len() as u16).to_be_bytes());
        packet.extend(value);
        packet
    }

    fn short_limits() -> Limits {
        Limits {
            dns: Duration::from_millis(100),
            datagram: Duration::from_millis(100),
            total: Duration::from_secs(1),
        }
    }

    #[test]
    fn published_rfc5769_ipv4_and_ipv6_response_vectors() {
        // RFC 5769 sections 2.2 / 2.3: independently specified wire examples,
        // including nonzero padding, MESSAGE-INTEGRITY and FINGERPRINT.
        let samples = [
            (
                concat!(
                    "0101003c2112a442b7e7a701bc34d686fa87dfae",
                    "8022000b7465737420766563746f7220",
                    "002000080001a147e112a643",
                    "000800142b91f599fd9e90c38c7489f92af9ba53f06be7d7",
                    "80280004c07d4c96"
                ),
                "192.0.2.1:32853",
            ),
            (
                concat!(
                    "010100482112a442b7e7a701bc34d686fa87dfae",
                    "8022000b7465737420766563746f7220",
                    "002000140002a1470113a9faa5d3f179bc25f4b5bed2b9d9",
                    "00080014a382954e4be67bf11784c97c8292c275bfe3ed41",
                    "80280004c8fb0b4c"
                ),
                "[2001:db8:1234:5678:11:2233:4455:6677]:32853",
            ),
        ];
        for (hex, expected) in samples {
            let bytes = hex
                .as_bytes()
                .chunks_exact(2)
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect::<Vec<_>>();
            let transaction = bytes[8..20].try_into().unwrap();
            assert_eq!(
                parse_response(&bytes, &transaction),
                Ok(expected.parse().unwrap())
            );
        }
    }

    #[test]
    fn xor_mapping_covers_both_address_families_and_transaction_mask() {
        let transaction = [0xab; 12];
        for address in ["203.0.113.9:54321", "[2001:db8::1234]:54321"] {
            let mapped: SocketAddr = address.parse().unwrap();
            assert_eq!(
                parse_response(&response(&transaction, mapped), &transaction),
                Ok(mapped)
            );
        }
    }

    #[test]
    fn rejects_wrong_transaction_cookie_lengths_and_truncated_attributes() {
        let transaction = [0x77; 12];
        let packet = response(&transaction, "203.0.113.9:12345".parse().unwrap());
        for position in [0, 2, 4, 8, 22, 24, 25] {
            let mut changed = packet.clone();
            changed[position] ^= 0x80;
            assert!(
                parse_response(&changed, &transaction).is_err(),
                "offset {position}"
            );
        }
        for length in 0..packet.len() {
            assert!(parse_response(&packet[..length], &transaction).is_err());
        }
        let mut trailing = packet.clone();
        trailing.extend_from_slice(&[0; 4]);
        assert!(parse_response(&trailing, &transaction).is_err());
    }

    #[test]
    fn checks_all_attributes_after_mapping_and_optional_fingerprint() {
        let transaction = [0x44; 12];
        let mut packet = response(&transaction, "203.0.113.9:12345".parse().unwrap());
        packet[2..4].copy_from_slice(&20_u16.to_be_bytes());
        let check = fingerprint(&packet);
        packet.extend_from_slice(&[0x80, 0x28, 0, 4]);
        packet.extend_from_slice(&check.to_be_bytes());
        assert!(parse_response(&packet, &transaction).is_ok());
        *packet.last_mut().unwrap() ^= 1;
        assert!(parse_response(&packet, &transaction).is_err());
        packet[32..36].copy_from_slice(&[0x00, 0x0f, 0, 4]);
        assert!(parse_response(&packet, &transaction).is_err());
    }

    #[test]
    fn duplicate_mapping_cannot_hide_conflicting_values() {
        let transaction = [0x33; 12];
        let mut packet = response(&transaction, "203.0.113.9:12345".parse().unwrap());
        let duplicate = packet[20..].to_vec();
        packet.extend(duplicate);
        packet[2..4].copy_from_slice(&24_u16.to_be_bytes());
        assert!(parse_response(&packet, &transaction).is_err());
    }

    #[test]
    fn coverage_requires_every_scheduled_observation_to_finish() {
        let mut observations = vec![observation(
            "test",
            StunFamily::Ipv4,
            StunStatus::Timeout,
            "timeout",
        )];
        assert_eq!(coverage(&[]), StunCoverage::Unknown);
        assert_eq!(coverage(&observations), StunCoverage::Unknown);
        observations.push(observation(
            "test",
            StunFamily::Ipv6,
            StunStatus::Completed,
            "",
        ));
        assert_eq!(coverage(&observations), StunCoverage::Partial);
        observations[0].status = StunStatus::Completed;
        assert_eq!(coverage(&observations), StunCoverage::Complete);
    }

    #[tokio::test]
    async fn local_udp_response_is_preserved_with_missing_family() {
        let server = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = server.local_addr().unwrap().port();
        let responder = tokio::spawn(async move {
            let mut bytes = [0; 100];
            let (len, peer) = server.recv_from(&mut bytes).await.unwrap();
            assert_eq!(len, HEADER_LEN);
            let transaction: [u8; 12] = bytes[8..20].try_into().unwrap();
            let packet = response(&transaction, "203.0.113.9:45678".parse().unwrap());
            server.send_to(&packet, peer).await.unwrap();
        });
        let report = probe_servers(&[("127.0.0.1", port)], short_limits()).await;
        responder.await.unwrap();
        assert_eq!(report.coverage, StunCoverage::Partial);
        assert_eq!(
            report.observations[0].mapped_address.as_deref(),
            Some("203.0.113.9")
        );
        assert_eq!(report.observations[0].mapped_port, Some(45678));
        assert_eq!(report.observations[1].status, StunStatus::NoAddress);
    }

    #[tokio::test]
    async fn local_udp_silence_is_unknown_and_does_not_outlive_deadline() {
        let server = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = server.local_addr().unwrap().port();
        let report = timeout_at(
            Instant::now() + Duration::from_secs(2),
            probe_servers(&[("127.0.0.1", port)], short_limits()),
        )
        .await
        .unwrap();
        assert_eq!(report.coverage, StunCoverage::Unknown);
        assert_eq!(report.observations[0].status, StunStatus::Timeout);
        assert!(report
            .observations
            .iter()
            .all(|item| item.mapped_address.is_none()));
    }

    #[tokio::test]
    async fn local_udp_wrong_transaction_is_protocol_error() {
        let server = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = server.local_addr().unwrap().port();
        let responder = tokio::spawn(async move {
            let mut bytes = [0; 100];
            let (_, peer) = server.recv_from(&mut bytes).await.unwrap();
            let mut transaction: [u8; 12] = bytes[8..20].try_into().unwrap();
            transaction[0] ^= 1;
            server
                .send_to(
                    &response(&transaction, "203.0.113.9:45678".parse().unwrap()),
                    peer,
                )
                .await
                .unwrap();
        });
        let report = probe_servers(&[("127.0.0.1", port)], short_limits()).await;
        responder.await.unwrap();
        assert_eq!(report.coverage, StunCoverage::Unknown);
        assert_eq!(report.observations[0].status, StunStatus::ProtocolError);
    }

    #[tokio::test]
    async fn connected_socket_ignores_a_response_from_another_source() {
        let server = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let rogue = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let peer = server.local_addr().unwrap();
        let responder = tokio::spawn(async move {
            let mut bytes = [0; 100];
            let (_, destination) = server.recv_from(&mut bytes).await.unwrap();
            let transaction: [u8; 12] = bytes[8..20].try_into().unwrap();
            rogue
                .send_to(
                    &response(&transaction, "203.0.113.99:45678".parse().unwrap()),
                    destination,
                )
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_millis(20)).await;
            server
                .send_to(
                    &response(&transaction, "203.0.113.9:45678".parse().unwrap()),
                    destination,
                )
                .await
                .unwrap();
        });
        let mapped = timeout_at(
            Instant::now() + Duration::from_secs(2),
            exchange(peer, StunFamily::Ipv4),
        )
        .await
        .unwrap()
        .unwrap();
        responder.await.unwrap();
        assert_eq!(mapped.ip(), "203.0.113.9".parse::<IpAddr>().unwrap());
    }
}
