pub mod lancache;
pub mod server;

use bytes::Bytes;
use reqwest::Client;

use crate::depot::ChunkId;
use crate::depot::DepotId;
use crate::depot::ManifestId;
use crate::error::Error;

use crate::cdn::server::CdnServer;

#[derive(Clone)]
pub struct CdnClient {
    http: Client,
    use_lancache: bool,
}

impl CdnClient {
    pub fn new() -> Result<Self, Error> {
        let http = Client::builder()
            .user_agent("Valve/Steam HTTP Client 1.0")
            .build()?;
        Ok(Self {
            http,
            use_lancache: false,
        })
    }

    pub fn with_lancache(mut self) -> Self {
        self.use_lancache = true;
        self
    }

    pub fn is_lancache(&self) -> bool {
        self.use_lancache
    }

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

        let resp = self.cdn_get(server, &path, cdn_auth_token).await?;
        Ok(resp)
    }

    pub async fn download_chunk(
        &self,
        server: &CdnServer,
        depot_id: DepotId,
        chunk_id: &ChunkId,
        cdn_auth_token: Option<&str>,
    ) -> Result<Bytes, Error> {
        let path = format!("depot/{}/chunk/{chunk_id}", depot_id.0);
        self.cdn_get(server, &path, cdn_auth_token).await
    }

    async fn cdn_get(
        &self,
        server: &CdnServer,
        path: &str,
        cdn_auth_token: Option<&str>,
    ) -> Result<Bytes, Error> {
        if self.use_lancache {
            let url = lancache::build_url(path, cdn_auth_token);
            let host = lancache::host_header(&server.vhost);
            tracing::debug!("Lancache request: {url} (Host: {host})");
            let resp = self
                .http
                .get(&url)
                .header("Host", host)
                .send()
                .await?
                .error_for_status()?;
            Ok(resp.bytes().await?)
        } else {
            let url = server.build_url(path, cdn_auth_token);
            let resp = self.http.get(&url).send().await?.error_for_status()?;
            Ok(resp.bytes().await?)
        }
    }
}
