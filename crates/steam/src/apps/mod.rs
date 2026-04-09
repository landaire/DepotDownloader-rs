//! Steam app/depot metadata queries (PICS, depot keys).

use prost::Message;

use crate::client::{LoggedIn, SteamClient};
use crate::client::msg::ClientMsg;
use crate::depot::{AppId, DepotId, DepotKey};
use crate::error::{ConnectionError, Error};
use crate::generated::{
    CMsgClientGetDepotDecryptionKey, CMsgClientGetDepotDecryptionKeyResponse,
    CMsgClientPicsAccessTokenRequest, CMsgClientPicsAccessTokenResponse,
    CMsgClientPicsProductInfoRequest, CMsgClientPicsProductInfoResponse,
    c_msg_client_pics_product_info_request,
};
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
                        Some(AccessToken {
                            app_id: AppId(t.appid?),
                            token: t.access_token?,
                        })
                    })
                    .collect());
            }
        }
    }

    /// Request PICS product info for apps (with access tokens).
    pub async fn pics_get_product_info(
        &self,
        apps: &[AccessToken],
    ) -> Result<Vec<AppInfo>, Error> {
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
                let body =
                    CMsgClientGetDepotDecryptionKeyResponse::decode(&incoming.body[..])?;

                let eresult = body.eresult;
                if eresult != Some(1) {
                    return Err(ConnectionError::LogonFailed {
                        eresult: eresult.unwrap_or(0),
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
}
