//! Steam CDN client for downloading depot manifests and content chunks.

pub mod server;

use bytes::Bytes;
use reqwest::Client;

use crate::depot::{ChunkId, DepotId, ManifestId};
use crate::error::Error;

use self::server::CdnServer;

/// A client for downloading content from Steam CDN servers.
#[derive(Clone)]
pub struct CdnClient {
    http: Client,
}

impl CdnClient {
    pub fn new() -> Result<Self, Error> {
        let http = Client::builder()
            .user_agent("Valve/Steam HTTP Client 1.0")
            .build()?;
        Ok(Self { http })
    }

    /// Download a depot manifest.
    ///
    /// Returns the raw manifest bytes (ZIP-compressed protobuf sections).
    pub async fn download_manifest(
        &self,
        server: &CdnServer,
        depot_id: DepotId,
        manifest_id: ManifestId,
        request_code: u64,
        cdn_auth_token: Option<&str>,
    ) -> Result<Bytes, Error> {
        let path = if request_code > 0 {
            format!(
                "depot/{}/manifest/{}/5/{request_code}",
                depot_id.0, manifest_id.0
            )
        } else {
            format!("depot/{}/manifest/{}/5", depot_id.0, manifest_id.0)
        };

        let url = server.build_url(&path, cdn_auth_token);
        let resp = self.http.get(&url).send().await?.error_for_status()?;
        Ok(resp.bytes().await?)
    }

    /// Download a single depot chunk.
    ///
    /// Returns the raw chunk bytes (encrypted + compressed).
    pub async fn download_chunk(
        &self,
        server: &CdnServer,
        depot_id: DepotId,
        chunk_id: &ChunkId,
        cdn_auth_token: Option<&str>,
    ) -> Result<Bytes, Error> {
        let path = format!("depot/{}/chunk/{chunk_id}", depot_id.0);
        let url = server.build_url(&path, cdn_auth_token);
        let resp = self.http.get(&url).send().await?.error_for_status()?;
        Ok(resp.bytes().await?)
    }
}
