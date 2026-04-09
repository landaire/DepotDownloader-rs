//! Steam authentication flows (credentials + QR code).
//!
//! Pre-logon auth methods operate on `SteamClient<Encrypted>` using
//! non-authenticated service method calls. Post-logon methods (guard code
//! submission, polling) operate on `SteamClient<LoggedIn>`.

use prost::Message;

use crate::client::{Encrypted, SteamClient};
use crate::error::Error;
use crate::generated::{
    CAuthenticationBeginAuthSessionViaCredentialsRequest,
    CAuthenticationBeginAuthSessionViaCredentialsResponse,
    CAuthenticationBeginAuthSessionViaQrRequest,
    CAuthenticationBeginAuthSessionViaQrResponse,
    CAuthenticationGetPasswordRsaPublicKeyResponse,
    CAuthenticationGetPasswordRsaPublicKeyRequest,
    CAuthenticationPollAuthSessionStatusRequest,
    CAuthenticationPollAuthSessionStatusResponse,
    CAuthenticationUpdateAuthSessionWithSteamGuardCodeRequest,
};

/// Guard type for 2FA.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardType {
    None,
    EmailCode,
    DeviceCode,
    DeviceConfirmation,
}

impl From<i32> for GuardType {
    fn from(v: i32) -> Self {
        match v {
            0 => Self::None,
            1 => Self::EmailCode,
            2 => Self::DeviceCode,
            3 => Self::DeviceConfirmation,
            _ => Self::None,
        }
    }
}

/// Result of beginning an auth session.
#[derive(Debug)]
pub struct AuthSession {
    pub client_id: Option<u64>,
    pub request_id: Option<Vec<u8>>,
    pub poll_interval: Option<f32>,
    pub allowed_confirmations: Vec<GuardType>,
    pub steam_id: Option<u64>,
}

/// Result of beginning a QR auth session.
#[derive(Debug)]
pub struct QrAuthSession {
    pub client_id: Option<u64>,
    pub request_id: Option<Vec<u8>>,
    pub challenge_url: Option<String>,
    pub poll_interval: Option<f32>,
    pub allowed_confirmations: Vec<GuardType>,
}

/// Tokens received from a successful auth poll.
#[derive(Debug, Clone)]
pub struct AuthTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub account_name: Option<String>,
}

// ── Pre-logon auth (Encrypted state, non-authed RPCs) ────────

impl SteamClient<Encrypted> {
    /// Get the RSA public key for encrypting a password.
    pub async fn get_password_rsa_public_key(
        &self,
        account_name: &str,
    ) -> Result<CAuthenticationGetPasswordRsaPublicKeyResponse, Error> {
        let encoded = CAuthenticationGetPasswordRsaPublicKeyRequest {
            account_name: Some(account_name.to_string()),
        }
        .encode_to_vec();

        let resp = self
            .call_service_method_non_authed(
                "Authentication.GetPasswordRSAPublicKey#1",
                &encoded,
            )
            .await?;

        Ok(CAuthenticationGetPasswordRsaPublicKeyResponse::decode(
            &resp.body[..],
        )?)
    }

    /// Begin a credential-based auth session.
    pub async fn begin_auth_session_via_credentials(
        &self,
        request: CAuthenticationBeginAuthSessionViaCredentialsRequest,
    ) -> Result<AuthSession, Error> {
        let encoded = request.encode_to_vec();
        let resp = self
            .call_service_method_non_authed(
                "Authentication.BeginAuthSessionViaCredentials#1",
                &encoded,
            )
            .await?;

        let body = CAuthenticationBeginAuthSessionViaCredentialsResponse::decode(
            &resp.body[..],
        )?;

        Ok(AuthSession {
            client_id: body.client_id,
            request_id: body.request_id,
            poll_interval: body.interval,
            allowed_confirmations: body
                .allowed_confirmations
                .iter()
                .filter_map(|c| c.confirmation_type.map(GuardType::from))
                .collect(),
            steam_id: body.steamid,
        })
    }

    /// Begin a QR-based auth session.
    pub async fn begin_auth_session_via_qr(
        &self,
        request: CAuthenticationBeginAuthSessionViaQrRequest,
    ) -> Result<QrAuthSession, Error> {
        let encoded = request.encode_to_vec();
        let resp = self
            .call_service_method_non_authed(
                "Authentication.BeginAuthSessionViaQR#1",
                &encoded,
            )
            .await?;

        let body =
            CAuthenticationBeginAuthSessionViaQrResponse::decode(&resp.body[..])?;

        Ok(QrAuthSession {
            client_id: body.client_id,
            request_id: body.request_id,
            challenge_url: body.challenge_url,
            poll_interval: body.interval,
            allowed_confirmations: body
                .allowed_confirmations
                .iter()
                .filter_map(|c| c.confirmation_type.map(GuardType::from))
                .collect(),
        })
    }

    /// Poll for auth session completion (pre-logon).
    pub async fn poll_auth_session(
        &self,
        client_id: u64,
        request_id: &[u8],
    ) -> Result<Option<AuthTokens>, Error> {
        let encoded = CAuthenticationPollAuthSessionStatusRequest {
            client_id: Some(client_id),
            request_id: Some(request_id.to_vec()),
            ..Default::default()
        }
        .encode_to_vec();

        let resp = self
            .call_service_method_non_authed(
                "Authentication.PollAuthSessionStatus#1",
                &encoded,
            )
            .await?;

        let body =
            CAuthenticationPollAuthSessionStatusResponse::decode(&resp.body[..])?;

        match (body.access_token, body.refresh_token) {
            (Some(access), Some(refresh)) if !access.is_empty() => {
                Ok(Some(AuthTokens {
                    access_token: access,
                    refresh_token: refresh,
                    account_name: body.account_name,
                }))
            }
            _ => Ok(None),
        }
    }

    /// Submit a SteamGuard code (email or authenticator) during pre-logon auth.
    pub async fn submit_steam_guard_code(
        &self,
        client_id: u64,
        steam_id: u64,
        code: &str,
        code_type: GuardType,
    ) -> Result<(), Error> {
        let guard_type = match code_type {
            GuardType::EmailCode => 1,
            GuardType::DeviceCode => 2,
            _ => 0,
        };

        let encoded = CAuthenticationUpdateAuthSessionWithSteamGuardCodeRequest {
            client_id: Some(client_id),
            steamid: Some(steam_id),
            code: Some(code.to_string()),
            code_type: Some(guard_type),
        }
        .encode_to_vec();

        let _resp = self
            .call_service_method_non_authed(
                "Authentication.UpdateAuthSessionWithSteamGuardCode#1",
                &encoded,
            )
            .await?;

        Ok(())
    }
}
