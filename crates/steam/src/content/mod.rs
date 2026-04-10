//! Content service methods: CDN server discovery, manifest request codes, CDN auth tokens.
//!
//! These are unified message RPCs via the `ContentServerDirectory` service.

use prost::Message;

use crate::cdn::server::CdnServer;
use crate::client::LoggedIn;
use crate::client::SteamClient;
use crate::depot::AppId;
use crate::depot::CellId;
use crate::depot::DepotId;
use crate::depot::ManifestId;
use crate::error::Error;
use crate::generated::CContentServerDirectoryGetCdnAuthTokenRequest;
use crate::generated::CContentServerDirectoryGetCdnAuthTokenResponse;
use crate::generated::CContentServerDirectoryGetManifestRequestCodeRequest;
use crate::generated::CContentServerDirectoryGetManifestRequestCodeResponse;
use crate::generated::CContentServerDirectoryGetServersForSteamPipeRequest;
use crate::generated::CContentServerDirectoryGetServersForSteamPipeResponse;
use crate::generated::CContentServerDirectoryServerInfo;

/// A CDN auth token with its expiration time.
#[derive(Debug, Clone)]
pub struct CdnAuthToken {
    pub token: Option<String>,
    pub expiration_time: Option<u32>,
}

impl SteamClient<LoggedIn> {
    /// Get a list of CDN servers for downloading content.
    pub async fn get_cdn_servers(
        &self,
        cell_id: CellId,
        max_servers: Option<u32>,
    ) -> Result<Vec<CdnServer>, Error> {
        let encoded = CContentServerDirectoryGetServersForSteamPipeRequest {
            cell_id: Some(cell_id.0),
            max_servers: max_servers.or(Some(20)),
            ip_override: None,
            launcher_type: None,
            ipv6_public: None,
            current_connections: Vec::new(),
        }
        .encode_to_vec();

        let resp = self
            .call_service_method("ContentServerDirectory.GetServersForSteamPipe#1", &encoded)
            .await?;

        let body = CContentServerDirectoryGetServersForSteamPipeResponse::decode(&resp.body[..])?;

        let servers: Vec<_> = body
            .servers
            .into_iter()
            .filter_map(|info| {
                let host = info.host.clone();
                let result = parse_server_info(info);
                if result.is_none() {
                    tracing::debug!("Skipping unparseable CDN server: {host:?}");
                }
                result
            })
            .collect();
        Ok(servers)
    }

    /// Get a manifest request code (time-limited, ~5 min validity).
    pub async fn get_manifest_request_code(
        &self,
        app_id: AppId,
        depot_id: DepotId,
        manifest_id: ManifestId,
        branch: Option<&str>,
        branch_password_hash: Option<&str>,
    ) -> Result<Option<u64>, Error> {
        let encoded = CContentServerDirectoryGetManifestRequestCodeRequest {
            app_id: Some(app_id.0),
            depot_id: Some(depot_id.0),
            manifest_id: Some(manifest_id.0),
            app_branch: branch.map(String::from),
            branch_password_hash: branch_password_hash.map(String::from),
        }
        .encode_to_vec();

        let resp = self
            .call_service_method("ContentServerDirectory.GetManifestRequestCode#1", &encoded)
            .await?;

        let body = CContentServerDirectoryGetManifestRequestCodeResponse::decode(&resp.body[..])?;

        Ok(body.manifest_request_code)
    }

    /// Get a CDN auth token for a specific depot and host.
    pub async fn get_cdn_auth_token(
        &self,
        app_id: AppId,
        depot_id: DepotId,
        host_name: &str,
    ) -> Result<CdnAuthToken, Error> {
        let encoded = CContentServerDirectoryGetCdnAuthTokenRequest {
            depot_id: Some(depot_id.0),
            host_name: Some(host_name.to_string()),
            app_id: Some(app_id.0),
        }
        .encode_to_vec();

        let resp = self
            .call_service_method("ContentServerDirectory.GetCDNAuthToken#1", &encoded)
            .await?;

        let body = CContentServerDirectoryGetCdnAuthTokenResponse::decode(&resp.body[..])?;

        Ok(CdnAuthToken {
            token: body.token,
            expiration_time: body.expiration_time,
        })
    }
}

/// Convert a protobuf server info into our CdnServer type.
fn parse_server_info(info: CContentServerDirectoryServerInfo) -> Option<CdnServer> {
    let host = info.host?;

    // Host may include a port (e.g., "cdn.example.com:443")
    let (hostname, port) = if let Some(idx) = host.rfind(':') {
        let port = host[idx + 1..].parse().ok()?;
        (host[..idx].to_string(), port)
    } else {
        let https = info
            .https_support
            .as_deref()
            .is_some_and(|s| s == "mandatory" || s == "optional");
        (host, if https { 443 } else { 80 })
    };

    let https = info
        .https_support
        .as_deref()
        .is_some_and(|s| s == "mandatory" || s == "optional");

    Some(CdnServer {
        host: hostname.clone(),
        port,
        https,
        vhost: info.vhost.unwrap_or(hostname),
    })
}
