pub mod encryption;
pub mod framing;

use std::net::SocketAddr;

/// A Steam CM server endpoint.
#[derive(Debug, Clone)]
pub struct CmServer {
    pub addr: CmServerAddr,
    pub protocol: Protocol,
}

/// Address for a CM server - either already resolved or a hostname to resolve.
#[derive(Debug, Clone)]
pub enum CmServerAddr {
    Resolved(SocketAddr),
    Dns { host: String, port: u16 },
}

/// Connection protocol to a CM server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Tcp,
    WebSocket,
}

/// Default CM servers (DNS hostnames, resolved at connect time).
///
/// These are used when the Steam Directory API is unavailable.
pub static DEFAULT_CM_SERVERS: &[CmServer] = &[
    CmServer {
        addr: CmServerAddr::Dns {
            host: String::new(), // populated at runtime, see default_cm_servers()
            port: 27017,
        },
        protocol: Protocol::Tcp,
    },
];

/// Get the default CM servers. Uses DNS hostnames that Steam resolves.
pub fn default_cm_servers() -> Vec<CmServer> {
    vec![
        CmServer {
            addr: CmServerAddr::Dns {
                host: "ext1-sea1.steamserver.net".into(),
                port: 27017,
            },
            protocol: Protocol::Tcp,
        },
        CmServer {
            addr: CmServerAddr::Dns {
                host: "ext1-iad1.steamserver.net".into(),
                port: 27017,
            },
            protocol: Protocol::Tcp,
        },
        CmServer {
            addr: CmServerAddr::Dns {
                host: "ext1-lax1.steamserver.net".into(),
                port: 27017,
            },
            protocol: Protocol::Tcp,
        },
        CmServer {
            addr: CmServerAddr::Dns {
                host: "ext1-ord1.steamserver.net".into(),
                port: 27017,
            },
            protocol: Protocol::Tcp,
        },
    ]
}

/// Fetch CM server list from the Steam Directory Web API.
///
/// Queries `https://api.steampowered.com/ISteamDirectory/GetCMListForConnect/v1/`
pub async fn fetch_cm_servers(
    http: &reqwest::Client,
    cell_id: u32,
) -> Result<Vec<CmServer>, crate::error::Error> {
    let url = format!(
        "https://api.steampowered.com/ISteamDirectory/GetCMListForConnect/v1/?cellid={cell_id}"
    );

    let resp: serde_json::Value = http.get(&url).send().await?.json().await?;

    let mut servers = Vec::new();

    if let Some(server_list) = resp["response"]["serverlist"].as_array() {
        for entry in server_list {
            let endpoint = match entry["endpoint"].as_str() {
                Some(e) => e,
                None => {
                    tracing::trace!("CM server entry missing endpoint, skipping: {entry}");
                    continue;
                }
            };

            let server_type = entry["type"].as_str().unwrap_or("");

            let server = match server_type {
                "netfilter" => parse_dns_server(endpoint, Protocol::Tcp),
                "websockets" => parse_dns_server(endpoint, Protocol::WebSocket),
                other => {
                    tracing::trace!("Unknown CM server type {other:?}, skipping");
                    continue;
                }
            };

            match server {
                Some(s) => servers.push(s),
                None => tracing::warn!("Failed to parse CM server endpoint: {endpoint}"),
            }
        }
    }

    Ok(servers)
}

/// Parse a "host:port" endpoint string into a CmServer.
fn parse_dns_server(endpoint: &str, protocol: Protocol) -> Option<CmServer> {
    let (host, port_str) = endpoint.rsplit_once(':')?;
    let port = port_str.parse().ok()?;

    Some(CmServer {
        addr: CmServerAddr::Dns {
            host: host.to_string(),
            port,
        },
        protocol,
    })
}
