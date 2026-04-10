//! Steam app/depot metadata queries (PICS, depot keys).

use prost::Message;

use crate::client::LoggedIn;
use crate::client::SteamClient;
use crate::client::msg::ClientMsg;
use crate::depot::AppId;
use crate::depot::DepotId;
use crate::depot::DepotKey;
use crate::error::ConnectionError;
use crate::error::Error;
use crate::generated::CMsgClientCheckAppBetaPassword;
use crate::generated::CMsgClientCheckAppBetaPasswordResponse;
use crate::generated::CMsgClientGetDepotDecryptionKey;
use crate::generated::CMsgClientGetDepotDecryptionKeyResponse;
use crate::generated::CMsgClientPicsAccessTokenRequest;
use crate::generated::CMsgClientPicsAccessTokenResponse;
use crate::generated::CMsgClientPicsProductInfoRequest;
use crate::generated::CMsgClientPicsProductInfoResponse;
use crate::generated::c_msg_client_pics_product_info_request;
use crate::messages::EMsg;

/// PICS access token for an app.
#[derive(Debug, Clone, Copy)]
pub struct AccessToken {
    pub app_id: AppId,
    pub token: u64,
}

/// Product info for an app, as returned by PICS.
#[derive(Debug, Clone)]
pub struct AppInfo {
    pub app_id: Option<AppId>,
    pub change_number: Option<u32>,
    /// Raw KeyValue data (binary format). Parse with [`crate::types::key_value`].
    pub kv_data: Option<Vec<u8>>,
}

impl SteamClient<LoggedIn> {
    /// Request PICS access tokens for a set of app IDs.
    pub async fn pics_get_access_tokens(
        &self,
        app_ids: &[AppId],
    ) -> Result<Vec<AccessToken>, Error> {
        let encoded = CMsgClientPicsAccessTokenRequest {
            appids: app_ids.iter().map(|id| id.0).collect(),
            packageids: Vec::new(),
        }
        .encode_to_vec();

        let msg = ClientMsg::with_body(EMsg::CLIENT_PICS_ACCESS_TOKEN_REQUEST, &encoded);
        self.send_msg(&msg).await?;

        loop {
            let incoming = self.recv_msg().await?;
            if incoming.emsg == EMsg::CLIENT_PICS_ACCESS_TOKEN_RESPONSE {
                let body = CMsgClientPicsAccessTokenResponse::decode(&incoming.body[..])?;
                return Ok(body
                    .app_access_tokens
                    .iter()
                    .filter_map(|t| {
                        let result = match (t.appid, t.access_token) {
                            (Some(id), Some(token)) => Some(AccessToken {
                                app_id: AppId(id),
                                token,
                            }),
                            _ => None,
                        };
                        if result.is_none() {
                            tracing::debug!(
                                "PICS access token entry missing fields: appid={:?} token={:?}",
                                t.appid,
                                t.access_token.is_some()
                            );
                        }
                        result
                    })
                    .collect());
            }
        }
    }

    /// Request PICS product info for apps (with access tokens).
    pub async fn pics_get_product_info(&self, apps: &[AccessToken]) -> Result<Vec<AppInfo>, Error> {
        let encoded = CMsgClientPicsProductInfoRequest {
            apps: apps
                .iter()
                .map(|t| c_msg_client_pics_product_info_request::AppInfo {
                    appid: Some(t.app_id.0),
                    access_token: Some(t.token),
                    only_public_obsolete: None,
                })
                .collect(),
            packages: Vec::new(),
            meta_data_only: Some(false),
            num_prev_failed: None,
            obsolete_supports_package_tokens: None,
            sequence_number: None,
            single_response: None,
        }
        .encode_to_vec();

        let msg = ClientMsg::with_body(EMsg::CLIENT_PICS_PRODUCT_INFO_REQUEST, &encoded);
        self.send_msg(&msg).await?;

        let mut results = Vec::new();
        loop {
            let incoming = self.recv_msg().await?;
            if incoming.emsg == EMsg::CLIENT_PICS_PRODUCT_INFO_RESPONSE {
                let body = CMsgClientPicsProductInfoResponse::decode(&incoming.body[..])?;

                let pending = body.response_pending.unwrap_or(false);

                for app in body.apps {
                    results.push(AppInfo {
                        app_id: app.appid.map(AppId),
                        change_number: app.change_number,
                        kv_data: app.buffer,
                    });
                }

                if !pending {
                    return Ok(results);
                }
            }
        }
    }

    /// Get the decryption key for a depot.
    pub async fn get_depot_decryption_key(
        &self,
        depot_id: DepotId,
        app_id: AppId,
    ) -> Result<DepotKey, Error> {
        let encoded = CMsgClientGetDepotDecryptionKey {
            depot_id: Some(depot_id.0),
            app_id: Some(app_id.0),
        }
        .encode_to_vec();

        let msg = ClientMsg::with_body(EMsg::CLIENT_GET_DEPOT_DECRYPTION_KEY, &encoded);
        self.send_msg(&msg).await?;

        loop {
            let incoming = self.recv_msg().await?;
            if incoming.emsg == EMsg::CLIENT_GET_DEPOT_DECRYPTION_KEY_RESPONSE {
                let body = CMsgClientGetDepotDecryptionKeyResponse::decode(&incoming.body[..])?;

                if let Err(e) = crate::enums::EResultError::from_i32(body.eresult.unwrap_or(0)) {
                    return Err(ConnectionError::DepotAccessDenied {
                        depot_id: depot_id.0,
                        error: e,
                    }
                    .into());
                }

                let key_bytes = body.depot_encryption_key.ok_or(
                    crate::error::CryptoError::InvalidKeyLength {
                        expected: 32,
                        actual: 0,
                    },
                )?;
                if key_bytes.len() != 32 {
                    return Err(crate::error::CryptoError::InvalidKeyLength {
                        expected: 32,
                        actual: key_bytes.len(),
                    }
                    .into());
                }

                let mut key = [0u8; 32];
                key.copy_from_slice(&key_bytes);
                return Ok(DepotKey(key));
            }
        }
    }

    /// Check a beta branch password. Returns the list of branches the password
    /// unlocks, with their decrypted names.
    pub async fn check_beta_password(
        &self,
        app_id: AppId,
        password: &str,
    ) -> Result<Vec<BetaBranch>, Error> {
        let encoded = CMsgClientCheckAppBetaPassword {
            app_id: Some(app_id.0),
            betapassword: Some(password.to_string()),
            language: None,
        }
        .encode_to_vec();

        let msg = ClientMsg::with_body(EMsg::CLIENT_CHECK_APP_BETA_PASSWORD, &encoded);
        self.send_msg(&msg).await?;

        loop {
            let incoming = self.recv_msg().await?;
            if incoming.emsg == EMsg::CLIENT_CHECK_APP_BETA_PASSWORD_RESPONSE {
                let body = CMsgClientCheckAppBetaPasswordResponse::decode(&incoming.body[..])?;

                if let Err(e) = crate::enums::EResultError::from_i32(body.eresult.unwrap_or(0)) {
                    return Err(ConnectionError::DepotAccessDenied {
                        depot_id: app_id.0,
                        error: e,
                    }
                    .into());
                }

                return Ok(body
                    .betapasswords
                    .into_iter()
                    .map(|b| BetaBranch {
                        name: b.betaname,
                        password: b.betapassword,
                        description: b.betadescription,
                    })
                    .collect());
            }
        }
    }
}

/// A beta branch unlocked by a password.
#[derive(Debug, Clone)]
pub struct BetaBranch {
    pub name: Option<String>,
    pub password: Option<String>,
    pub description: Option<String>,
}
