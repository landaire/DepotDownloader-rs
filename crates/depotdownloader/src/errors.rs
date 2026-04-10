use steam::error::ConnectionError;
use steam::error::Error as SteamError;

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("{0}")]
    Steam(#[from] SteamError),

    #[error("{0}")]
    Manifest(#[from] steam::error::ManifestError),

    #[error("{0}")]
    Chunk(#[from] steam::depot::chunk::ChunkError),

    #[error("{0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Json(#[from] serde_json::Error),

    #[error("{0}")]
    Join(#[from] tokio::task::JoinError),

    #[error("{0}")]
    Reqwest(#[from] reqwest::Error),

    #[error("{0}")]
    Protobuf(#[from] prost::DecodeError),

    #[error("{0}")]
    Regex(#[from] regex::Error),

    #[error("{0}")]
    Other(String),
}

impl From<String> for CliError {
    fn from(s: String) -> Self {
        Self::Other(s)
    }
}

impl From<&str> for CliError {
    fn from(s: &str) -> Self {
        Self::Other(s.to_string())
    }
}

impl From<Box<dyn std::error::Error + Send + Sync>> for CliError {
    fn from(e: Box<dyn std::error::Error + Send + Sync>) -> Self {
        Self::Other(e.to_string())
    }
}

impl CliError {
    pub fn human_message(&self) -> Option<&'static str> {
        match self {
            Self::Steam(SteamError::Http(e)) => match e.status().map(|s| s.as_u16()) {
                Some(401) => Some(
                    "Access denied. This depot requires authentication.\n\
                         Try logging in with --username <user> or check that your account owns this app.",
                ),
                Some(403) => Some(
                    "Forbidden. The CDN rejected the request.\n\
                         The manifest request code may have expired. Try running the command again.",
                ),
                Some(404) => Some(
                    "Not found. The requested content does not exist on the CDN.\n\
                         The manifest ID may be wrong, or the content has been removed.",
                ),
                _ => None,
            },
            Self::Steam(SteamError::Connection(ConnectionError::DepotAccessDenied { .. })) => Some(
                "Access denied by Steam for this depot.\n\
                 If this is a paid game, log in with --username <user>.\n\
                 If this is a free dedicated server, check the depot ID.",
            ),
            Self::Steam(SteamError::Connection(ConnectionError::LogonFailed(_))) => Some(
                "Login failed. Check your credentials and try again.\n\
                 If using stored credentials, delete ~/.depotdownloader/tokens.json to re-authenticate.",
            ),
            Self::Steam(SteamError::Connection(ConnectionError::EncryptionFailed(_))) => Some(
                "Encryption handshake failed. The Steam server rejected our connection.\n\
                 Try again - a different server may be selected.",
            ),
            Self::Steam(SteamError::Connection(ConnectionError::Disconnected)) => Some(
                "Disconnected from the Steam server.\n\
                 The server may be busy. Try again in a moment.",
            ),
            Self::Steam(SteamError::Connection(ConnectionError::DnsResolutionFailed {
                ..
            })) => Some(
                "Could not resolve the server hostname.\n\
                 Check your network connection and DNS settings.",
            ),
            Self::Steam(SteamError::Io(e)) if e.kind() == std::io::ErrorKind::TimedOut => Some(
                "Connection timed out. The server may be unreachable.\n\
                 Check your network connection and try again.",
            ),
            Self::Steam(SteamError::Io(e)) if e.kind() == std::io::ErrorKind::ConnectionReset => {
                Some(
                    "Connection reset by the Steam server.\n\
                 Try again - a different server may be selected.",
                )
            }
            _ => None,
        }
    }

    pub fn print(&self, raw: bool) {
        if raw {
            eprintln!("Error: {self:?}");
            return;
        }

        if let Some(msg) = self.human_message() {
            eprintln!("Error: {msg}");
            eprintln!();
            eprintln!("(run with --raw-errors for technical details)");
        } else {
            eprintln!("Error: {self}");
        }
    }
}
