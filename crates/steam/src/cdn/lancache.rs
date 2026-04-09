use std::net::IpAddr;

const TRIGGER_DOMAIN: &str = "lancache.steamcontent.com";

/// Detect a lancache server on the local network by checking if
/// `lancache.steamcontent.com` resolves to a private RFC 1918 address.
pub async fn detect() -> bool {
    match tokio::net::lookup_host(format!("{TRIGGER_DOMAIN}:80")).await {
        Ok(addrs) => {
            for addr in addrs {
                if is_private(addr.ip()) {
                    tracing::info!("Detected lancache server at {}", addr.ip());
                    return true;
                }
            }
            false
        }
        Err(_) => false,
    }
}

/// Build a lancache-routed URL. Requests go to `lancache.steamcontent.com:80`
/// with the real CDN hostname in the `Host` header.
pub fn build_url(path: &str, cdn_auth_token: Option<&str>) -> String {
    match cdn_auth_token {
        Some(token) => format!("http://{TRIGGER_DOMAIN}/{path}?{token}"),
        None => format!("http://{TRIGGER_DOMAIN}/{path}"),
    }
}

/// The hostname to set in the `Host` header when using lancache.
pub fn host_header(vhost: &str) -> String {
    vhost.to_string()
}

fn is_private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.octets()[0] == 10
                || (v4.octets()[0] == 172 && (16..32).contains(&v4.octets()[1]))
                || (v4.octets()[0] == 192 && v4.octets()[1] == 168)
        }
        IpAddr::V6(v6) => {
            v6.is_loopback() || (v6.octets()[0] & 0xFE) == 0xFC
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn private_addresses() {
        assert!(is_private(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(is_private(IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
        assert!(is_private(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
        assert!(is_private(IpAddr::V4(Ipv4Addr::LOCALHOST)));
        assert!(!is_private(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
        assert!(!is_private(IpAddr::V4(Ipv4Addr::new(172, 32, 0, 1))));
    }
}
