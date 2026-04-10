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
            Self::Steam(SteamError::Connection(ConnectionError::ServiceMethodFailed(e))) => {
                use steam::enums::EResultError;
                match e {
                    EResultError::InvalidPassword => {
                        Some("Invalid password. Check your credentials and try again.")
                    }
                    EResultError::TwoFactorRequired => Some(
                        "This account requires two-factor authentication.\n\
                         A Steam Guard code will be requested during login.",
                    ),
                    EResultError::RateLimitExceeded | EResultError::LoginDeniedThrottle => {
                        Some("Too many login attempts. Wait a few minutes and try again.")
                    }
                    EResultError::Expired => Some("Session expired. Try logging in again."),
                    _ => None,
                }
            }
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

    pub fn is_auth_error(&self) -> bool {
        matches!(
            self,
            Self::Steam(SteamError::Connection(
                ConnectionError::LogonFailed(_) | ConnectionError::ServiceMethodFailed(_)
            ))
        )
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

#[cfg(test)]
mod tests {
    use super::*;
    use steam::enums::EResultError;

    fn make_service_error(e: EResultError) -> CliError {
        CliError::Steam(SteamError::Connection(
            ConnectionError::ServiceMethodFailed(e),
        ))
    }

    fn make_logon_error(e: EResultError) -> CliError {
        CliError::Steam(SteamError::Connection(ConnectionError::LogonFailed(e)))
    }

    #[test]
    fn service_method_failed_is_auth_error() {
        assert!(make_service_error(EResultError::InvalidPassword).is_auth_error());
        assert!(make_service_error(EResultError::Expired).is_auth_error());
        assert!(make_service_error(EResultError::TwoFactorRequired).is_auth_error());
    }

    #[test]
    fn logon_failed_is_auth_error() {
        assert!(make_logon_error(EResultError::InvalidPassword).is_auth_error());
        assert!(make_logon_error(EResultError::AccessDenied).is_auth_error());
    }

    #[test]
    fn connection_errors_are_not_auth_errors() {
        let err = CliError::Steam(SteamError::Connection(ConnectionError::Disconnected));
        assert!(!err.is_auth_error());

        let err = CliError::Steam(SteamError::Connection(
            ConnectionError::DnsResolutionFailed {
                host: "example.com".into(),
            },
        ));
        assert!(!err.is_auth_error());
    }

    #[test]
    fn io_error_is_not_auth_error() {
        let err = CliError::Io(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "refused",
        ));
        assert!(!err.is_auth_error());
    }

    #[test]
    fn other_error_is_not_auth_error() {
        let err = CliError::Other("something went wrong".into());
        assert!(!err.is_auth_error());
    }

    #[test]
    fn human_message_for_invalid_password() {
        let err = make_service_error(EResultError::InvalidPassword);
        let msg = err.human_message().unwrap();
        assert!(msg.contains("Invalid password"));
    }

    #[test]
    fn human_message_for_rate_limit() {
        let err = make_service_error(EResultError::RateLimitExceeded);
        let msg = err.human_message().unwrap();
        assert!(msg.contains("Too many login attempts"));

        let err = make_service_error(EResultError::LoginDeniedThrottle);
        let msg = err.human_message().unwrap();
        assert!(msg.contains("Too many login attempts"));
    }

    #[test]
    fn human_message_for_logon_failed() {
        let err = make_logon_error(EResultError::AccessDenied);
        let msg = err.human_message().unwrap();
        assert!(msg.contains("Login failed"));
    }

    #[test]
    fn human_message_for_expired_session() {
        let err = make_service_error(EResultError::Expired);
        let msg = err.human_message().unwrap();
        assert!(msg.contains("Session expired"));
    }

    #[test]
    fn human_message_for_two_factor() {
        let err = make_service_error(EResultError::TwoFactorRequired);
        let msg = err.human_message().unwrap();
        assert!(msg.contains("two-factor"));
    }

    #[test]
    fn human_message_for_encryption_failed() {
        let err = CliError::Steam(SteamError::Connection(ConnectionError::EncryptionFailed(
            EResultError::Fail,
        )));
        let msg = err.human_message().unwrap();
        assert!(msg.contains("Encryption handshake"));
    }

    #[test]
    fn human_message_for_disconnected() {
        let err = CliError::Steam(SteamError::Connection(ConnectionError::Disconnected));
        let msg = err.human_message().unwrap();
        assert!(msg.contains("Disconnected"));
    }

    #[test]
    fn human_message_for_depot_access_denied() {
        let err = CliError::Steam(SteamError::Connection(ConnectionError::DepotAccessDenied {
            depot_id: 12345,
            error: EResultError::AccessDenied,
        }));
        let msg = err.human_message().unwrap();
        assert!(msg.contains("Access denied"));
    }

    #[test]
    fn human_message_none_for_unknown_service_error() {
        let err = make_service_error(EResultError::Unknown(9999));
        assert!(err.human_message().is_none());
    }

    #[test]
    fn human_message_none_for_other() {
        let err = CliError::Other("something".into());
        assert!(err.human_message().is_none());
    }
}
