//! 最小 STUN Binding 探测（RFC 5389）：判断 TURN 端口 UDP 是否存活应答。
//! 用途：目录对节点自报的 TURN 地址做**中继能力分级**（relay_capable）。
//! 口径：STUN Binding 应答 = TURN 端口 UDP 存活（coturn 同端口开 STUN）；
//! 不证明 Allocate/凭证有效性——那需要完整 TURN 客户端，后续按需升级。

use std::net::UdpSocket;
use std::time::Duration;

/// 解析 `turn:host:port` / `stun:host:port` / `turns:host:port` → (host, port)。
pub fn parse_turn_host_port(url: &str) -> Option<(String, u16)> {
    let rest = url.trim();
    let rest = if let Some(r) = rest.strip_prefix("turns:") {
        r
    } else if let Some(r) = rest.strip_prefix("turn:") {
        r
    } else if let Some(r) = rest.strip_prefix("stun:") {
        r
    } else {
        rest
    };
    let (host, port) = rest.rsplit_once(':')?;
    let port: u16 = port.parse().ok()?;
    if host.is_empty() {
        return None;
    }
    Some((host.to_owned(), port))
}

/// 发一个 Binding Request（12 字节 transaction id），3 秒内收到
/// 匹配 txid 的 Binding Success Response 即认为端口存活。
pub fn binding_probe(host: &str, port: u16) -> bool {
    let mut pkt = [0u8; 20];
    pkt[1] = 0x01; // Binding Request
    pkt[4..8].copy_from_slice(&0x2112_A442u32.to_be_bytes()); // magic cookie
                                                              // txid：非密码学随机（探测去重足够；真随机需求见凭证路径，非此处）
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    pkt[8..12].copy_from_slice(&nanos.to_be_bytes());
    pkt[12..16].copy_from_slice(&std::process::id().to_be_bytes());
    pkt[16..20].copy_from_slice(&0x4843_5531u32.to_be_bytes()); // "HCU1"

    let Ok(sock) = UdpSocket::bind("0.0.0.0:0") else {
        return false;
    };
    if sock.connect((host, port)).is_err() {
        return false;
    }
    if sock.send(&pkt).is_err() {
        return false;
    }
    let _ = sock.set_read_timeout(Some(Duration::from_secs(3)));
    let mut buf = [0u8; 128];
    let Ok(n) = sock.recv(&mut buf) else {
        return false;
    };
    n >= 20 && buf[0] == 0x01 && buf[1] == 0x01 && buf[8..20] == pkt[8..20]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_turn_urls() {
        assert_eq!(
            parse_turn_host_port("turn:turn.example.com:3478"),
            Some(("turn.example.com".into(), 3478))
        );
        assert_eq!(
            parse_turn_host_port("stun:1.2.3.4:5444"),
            Some(("1.2.3.4".into(), 5444))
        );
        assert_eq!(
            parse_turn_host_port("turns:relay.example.com:443"),
            Some(("relay.example.com".into(), 443))
        );
        assert_eq!(parse_turn_host_port("turn:noport"), None);
        assert_eq!(parse_turn_host_port("garbage"), None);
    }

    #[test]
    fn binding_probe_against_mock_responder() {
        // mock STUN：收 20B 请求，回 Success Response（同 txid）
        let sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = sock.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let mut buf = [0u8; 64];
            if let Ok((n, peer)) = sock.recv_from(&mut buf) {
                if n >= 20 {
                    let mut resp = buf;
                    resp[0] = 0x01;
                    resp[1] = 0x01; // Binding Success
                    let _ = sock.send_to(&resp[..20], peer);
                }
            }
        });
        assert!(
            binding_probe("127.0.0.1", port),
            "mock responder should answer"
        );
    }

    #[test]
    fn binding_probe_timeout_on_silent_port() {
        // 绑定但永不应答 → 3 秒超时判 false
        let sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = sock.local_addr().unwrap().port();
        std::mem::forget(sock); // 保持端口占用但不响应
        assert!(!binding_probe("127.0.0.1", port), "silent port must fail");
    }
}
