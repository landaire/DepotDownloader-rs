/// A Steam CDN server endpoint.
#[derive(Debug, Clone)]
pub struct CdnServer {
    pub host: String,
    pub port: u16,
    pub https: bool,
    pub vhost: String,
}

impl CdnServer {
    /// Build a full URL for a CDN request path.
    pub fn build_url(&self, path: &str, cdn_auth_token: Option<&str>) -> String {
        let scheme = if self.https { "https" } else { "http" };
        match cdn_auth_token {
            Some(token) => format!("{scheme}://{}:{}/{path}?{token}", self.vhost, self.port),
            None => format!("{scheme}://{}:{}/{path}", self.vhost, self.port),
        }
    }
}
