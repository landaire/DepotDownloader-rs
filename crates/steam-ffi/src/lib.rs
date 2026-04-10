enum SteamClientInner {
    Encrypted(steam::client::SteamClient<steam::client::Encrypted>),
    LoggedIn(steam::client::SteamClient<steam::client::LoggedIn>),
}

impl SteamClientInner {
    fn as_encrypted(
        &self,
    ) -> Result<&steam::client::SteamClient<steam::client::Encrypted>, String> {
        match self {
            SteamClientInner::Encrypted(c) => Ok(c),
            SteamClientInner::LoggedIn(_) => Err("client is already logged in".into()),
        }
    }

    fn as_logged_in(&self) -> Result<&steam::client::SteamClient<steam::client::LoggedIn>, String> {
        match self {
            SteamClientInner::LoggedIn(c) => Ok(c),
            SteamClientInner::Encrypted(_) => Err("client is not logged in".into()),
        }
    }
}

#[diplomat::bridge]
mod ffi {
    use diplomat_runtime::DiplomatWrite;
    use std::fmt::Write as _;

    use super::SteamClientInner;
    use base64::Engine as _;

    #[diplomat::opaque]
    pub struct AppId(steam::depot::AppId);

    impl AppId {
        pub fn new(id: u32) -> Box<AppId> {
            Box::new(AppId(steam::depot::AppId(id)))
        }

        pub fn value(&self) -> u32 {
            self.0.0
        }
    }

    #[diplomat::opaque]
    pub struct DepotId(steam::depot::DepotId);

    impl DepotId {
        pub fn new(id: u32) -> Box<DepotId> {
            Box::new(DepotId(steam::depot::DepotId(id)))
        }

        pub fn value(&self) -> u32 {
            self.0.0
        }
    }

    #[diplomat::opaque]
    pub struct ManifestId(steam::depot::ManifestId);

    impl ManifestId {
        pub fn new(id: u64) -> Box<ManifestId> {
            Box::new(ManifestId(steam::depot::ManifestId(id)))
        }

        pub fn value(&self) -> u64 {
            self.0.0
        }
    }

    #[diplomat::opaque]
    pub struct CellId(steam::depot::CellId);

    impl CellId {
        pub fn new(id: u32) -> Box<CellId> {
            Box::new(CellId(steam::depot::CellId(id)))
        }

        pub fn value(&self) -> u32 {
            self.0.0
        }
    }

    pub enum SteamError {
        Invalid,
        Fail,
        NoConnection,
        InvalidPassword,
        LoggedInElsewhere,
        InvalidProtocolVer,
        InvalidParam,
        FileNotFound,
        Busy,
        InvalidState,
        AccessDenied,
        Timeout,
        Banned,
        AccountNotFound,
        ServiceUnavailable,
        NotLoggedOn,
        Expired,
        RateLimitExceeded,
        TwoFactorRequired,
        Unknown,
    }

    impl SteamError {
        #[diplomat::attr(auto, constructor)]
        pub fn from_eresult(code: i32) -> SteamError {
            match code {
                0 => SteamError::Invalid,
                2 => SteamError::Fail,
                3 => SteamError::NoConnection,
                5 => SteamError::InvalidPassword,
                6 => SteamError::LoggedInElsewhere,
                7 => SteamError::InvalidProtocolVer,
                8 => SteamError::InvalidParam,
                9 => SteamError::FileNotFound,
                10 => SteamError::Busy,
                11 => SteamError::InvalidState,
                15 => SteamError::AccessDenied,
                16 => SteamError::Timeout,
                17 => SteamError::Banned,
                18 => SteamError::AccountNotFound,
                20 => SteamError::ServiceUnavailable,
                21 => SteamError::NotLoggedOn,
                27 => SteamError::Expired,
                84 => SteamError::RateLimitExceeded,
                85 => SteamError::TwoFactorRequired,
                _ => SteamError::Unknown,
            }
        }
    }

    pub enum GuardType {
        None,
        EmailCode,
        DeviceCode,
        DeviceConfirmation,
    }

    #[diplomat::opaque]
    pub struct FfiError(String);

    impl FfiError {
        pub fn message(&self, write: &mut DiplomatWrite) {
            let _ = write!(write, "{}", self.0);
        }
    }

    #[diplomat::opaque]
    // Field is held for its Drop impl (shuts down the runtime)
    #[allow(dead_code)]
    pub struct Runtime(tokio::runtime::Runtime);

    impl Runtime {
        pub fn new() -> Result<Box<Runtime>, Box<FfiError>> {
            tokio::runtime::Runtime::new()
                .map(|rt| Box::new(Runtime(rt)))
                .map_err(|e| Box::new(FfiError(e.to_string())))
        }
    }

    #[diplomat::opaque]
    pub struct CmServerList(Vec<steam::connection::CmServer>);

    impl CmServerList {
        pub fn fetch(rt: &Runtime, cell_id: &CellId) -> Result<Box<CmServerList>, Box<FfiError>> {
            rt.0.block_on(async {
                let http = reqwest::Client::new();
                let servers = steam::connection::fetch_cm_servers(&http, cell_id.0)
                    .await
                    .map_err(|e| Box::new(FfiError(e.to_string())))?;
                let tcp_servers: Vec<_> = servers
                    .into_iter()
                    .filter(|s| s.protocol == steam::connection::Protocol::Tcp)
                    .collect();
                Ok(Box::new(CmServerList(tcp_servers)))
            })
        }

        pub fn defaults() -> Box<CmServerList> {
            Box::new(CmServerList(steam::connection::default_cm_servers()))
        }

        pub fn len(&self) -> u32 {
            self.0.len() as u32
        }
    }

    #[diplomat::opaque_mut]
    pub struct SteamClient(Option<SteamClientInner>);

    impl SteamClient {
        pub fn connect(
            rt: &Runtime,
            servers: &CmServerList,
            server_index: u32,
        ) -> Result<Box<SteamClient>, Box<FfiError>> {
            let server = servers
                .0
                .get(server_index as usize)
                .ok_or_else(|| Box::new(FfiError("server index out of bounds".into())))?;

            rt.0.block_on(async {
                let (client, _events) = steam::client::DisconnectedClient::new();
                let connected = client
                    .connect_tcp(server)
                    .await
                    .map_err(|e| Box::new(FfiError(e.to_string())))?;
                let encrypted = connected
                    .encrypt()
                    .await
                    .map_err(|e| Box::new(FfiError(e.to_string())))?;
                Ok(Box::new(SteamClient(Some(SteamClientInner::Encrypted(
                    encrypted,
                )))))
            })
        }

        pub fn login_anonymous(
            &mut self,
            rt: &Runtime,
            cell_id: &CellId,
        ) -> Result<(), Box<FfiError>> {
            let inner = self
                .0
                .take()
                .ok_or_else(|| Box::new(FfiError("client consumed".into())))?;
            let SteamClientInner::Encrypted(client) = inner else {
                return Err(Box::new(FfiError(
                    "must be in encrypted state to login".into(),
                )));
            };

            rt.0.block_on(async {
                let logon = steam::generated::CMsgClientLogon {
                    protocol_version: Some(steam::client::PROTOCOL_VERSION),
                    cell_id: Some(cell_id.0.0),
                    client_os_type: Some(steam::enums::EOSType::Windows11 as u32),
                    client_language: Some("english".to_string()),
                    ..Default::default()
                };
                let logon_body = prost::Message::encode_to_vec(&logon);
                let mut logon_msg = steam::client::msg::ClientMsg::with_body(
                    steam::messages::EMsg::CLIENT_LOGON,
                    &logon_body,
                );

                let anon_id = steam::types::SteamId::from_parts(
                    steam::enums::EUniverse::Public as u8,
                    steam::enums::EAccountType::AnonUser as u8,
                    0,
                    0,
                );
                logon_msg.header.steamid = Some(anon_id.raw());
                logon_msg.header.client_sessionid = Some(0);

                let (logged_in, _resp) = client
                    .login(logon_msg)
                    .await
                    .map_err(|e| Box::new(FfiError(e.to_string())))?;
                self.0 = Some(SteamClientInner::LoggedIn(logged_in));
                Ok(())
            })
        }

        pub fn login_with_token(
            &mut self,
            rt: &Runtime,
            username: &DiplomatStr,
            access_token: &DiplomatStr,
            cell_id: &CellId,
        ) -> Result<(), Box<FfiError>> {
            let inner = self
                .0
                .take()
                .ok_or_else(|| Box::new(FfiError("client consumed".into())))?;
            let SteamClientInner::Encrypted(client) = inner else {
                return Err(Box::new(FfiError(
                    "must be in encrypted state to login".into(),
                )));
            };

            let username = std::str::from_utf8(username)
                .map_err(|e| Box::new(FfiError(e.to_string())))?
                .to_string();
            let token = std::str::from_utf8(access_token)
                .map_err(|e| Box::new(FfiError(e.to_string())))?
                .to_string();

            rt.0.block_on(async {
                let logon = steam::generated::CMsgClientLogon {
                    protocol_version: Some(steam::client::PROTOCOL_VERSION),
                    cell_id: Some(cell_id.0.0),
                    client_os_type: Some(steam::enums::EOSType::Windows11 as u32),
                    client_language: Some("english".to_string()),
                    access_token: Some(token),
                    account_name: Some(username),
                    ..Default::default()
                };
                let logon_body = prost::Message::encode_to_vec(&logon);
                let mut logon_msg = steam::client::msg::ClientMsg::with_body(
                    steam::messages::EMsg::CLIENT_LOGON,
                    &logon_body,
                );

                let user_id = steam::types::SteamId::from_parts(
                    steam::enums::EUniverse::Public as u8,
                    steam::enums::EAccountType::Individual as u8,
                    0,
                    0,
                );
                logon_msg.header.steamid = Some(user_id.raw());
                logon_msg.header.client_sessionid = Some(0);

                let (logged_in, _resp) = client
                    .login(logon_msg)
                    .await
                    .map_err(|e| Box::new(FfiError(e.to_string())))?;
                self.0 = Some(SteamClientInner::LoggedIn(logged_in));
                Ok(())
            })
        }

        pub fn get_rsa_public_key(
            &self,
            rt: &Runtime,
            username: &DiplomatStr,
        ) -> Result<Box<RsaPublicKey>, Box<FfiError>> {
            let client = self
                .0
                .as_ref()
                .ok_or_else(|| Box::new(FfiError("client consumed".into())))?
                .as_encrypted()
                .map_err(|e| Box::new(FfiError(e)))?;
            let username =
                std::str::from_utf8(username).map_err(|e| Box::new(FfiError(e.to_string())))?;

            rt.0.block_on(async {
                let resp = client
                    .get_password_rsa_public_key(username)
                    .await
                    .map_err(|e| Box::new(FfiError(e.to_string())))?;
                Ok(Box::new(RsaPublicKey {
                    modulus: resp.publickey_mod.unwrap_or_default(),
                    exponent: resp.publickey_exp.unwrap_or_default(),
                    timestamp: resp.timestamp.unwrap_or(0),
                }))
            })
        }

        pub fn begin_auth_credentials(
            &self,
            rt: &Runtime,
            username: &DiplomatStr,
            encrypted_password: &[u8],
            timestamp: u64,
            device_name: &DiplomatStr,
        ) -> Result<Box<AuthSession>, Box<FfiError>> {
            let client = self
                .0
                .as_ref()
                .ok_or_else(|| Box::new(FfiError("client consumed".into())))?
                .as_encrypted()
                .map_err(|e| Box::new(FfiError(e)))?;
            let username =
                std::str::from_utf8(username).map_err(|e| Box::new(FfiError(e.to_string())))?;
            let device_name =
                std::str::from_utf8(device_name).map_err(|e| Box::new(FfiError(e.to_string())))?;
            let password_b64 = base64::engine::general_purpose::STANDARD.encode(encrypted_password);

            rt.0.block_on(async {
                let request =
                    steam::generated::CAuthenticationBeginAuthSessionViaCredentialsRequest {
                        account_name: Some(username.to_string()),
                        encrypted_password: Some(password_b64),
                        encryption_timestamp: Some(timestamp),
                        persistence: Some(steam::enums::ESessionPersistence::Persistent as i32),
                        website_id: Some("Client".to_string()),
                        device_details: Some(steam::generated::CAuthenticationDeviceDetails {
                            device_friendly_name: Some(device_name.to_string()),
                            platform_type: Some(
                                steam::enums::EAuthTokenPlatformType::SteamClient as i32,
                            ),
                            os_type: Some(steam::enums::EOSType::Windows11 as i32),
                            ..Default::default()
                        }),
                        ..Default::default()
                    };

                let session = client
                    .begin_auth_session_via_credentials(request)
                    .await
                    .map_err(|e| Box::new(FfiError(e.to_string())))?;

                Ok(Box::new(AuthSession(session)))
            })
        }

        pub fn poll_auth(
            &self,
            rt: &Runtime,
            session: &AuthSession,
        ) -> Result<Box<AuthTokens>, Box<FfiError>> {
            let client = self
                .0
                .as_ref()
                .ok_or_else(|| Box::new(FfiError("client consumed".into())))?
                .as_encrypted()
                .map_err(|e| Box::new(FfiError(e)))?;
            let client_id = session
                .0
                .client_id
                .ok_or_else(|| Box::new(FfiError("missing client_id".into())))?;
            let request_id = session
                .0
                .request_id
                .as_ref()
                .ok_or_else(|| Box::new(FfiError("missing request_id".into())))?;

            rt.0.block_on(async {
                match client
                    .poll_auth_session(client_id, request_id)
                    .await
                    .map_err(|e| Box::new(FfiError(e.to_string())))?
                {
                    Some(tokens) => Ok(Box::new(AuthTokens(tokens))),
                    None => Err(Box::new(FfiError("auth session pending".into()))),
                }
            })
        }

        pub fn submit_guard_code(
            &self,
            rt: &Runtime,
            session: &AuthSession,
            code: &DiplomatStr,
            code_type: GuardType,
        ) -> Result<(), Box<FfiError>> {
            let client = self
                .0
                .as_ref()
                .ok_or_else(|| Box::new(FfiError("client consumed".into())))?
                .as_encrypted()
                .map_err(|e| Box::new(FfiError(e)))?;
            let client_id = session
                .0
                .client_id
                .ok_or_else(|| Box::new(FfiError("missing client_id".into())))?;
            let steam_id = session
                .0
                .steam_id
                .ok_or_else(|| Box::new(FfiError("missing steam_id".into())))?;
            let code = std::str::from_utf8(code).map_err(|e| Box::new(FfiError(e.to_string())))?;
            let guard_type = match code_type {
                GuardType::EmailCode => steam::auth::GuardType::EmailCode,
                GuardType::DeviceCode => steam::auth::GuardType::DeviceCode,
                _ => steam::auth::GuardType::None,
            };

            rt.0.block_on(async {
                client
                    .submit_steam_guard_code(client_id, steam_id, code, guard_type)
                    .await
                    .map_err(|e| Box::new(FfiError(e.to_string())))
            })
        }

        pub fn get_access_tokens(
            &self,
            rt: &Runtime,
            app_ids: &[u32],
        ) -> Result<Box<AccessTokenList>, Box<FfiError>> {
            let client = self
                .0
                .as_ref()
                .ok_or_else(|| Box::new(FfiError("client consumed".into())))?
                .as_logged_in()
                .map_err(|e| Box::new(FfiError(e)))?;
            let ids: Vec<steam::depot::AppId> =
                app_ids.iter().map(|&id| steam::depot::AppId(id)).collect();

            rt.0.block_on(async {
                let tokens = client
                    .pics_get_access_tokens(&ids)
                    .await
                    .map_err(|e| Box::new(FfiError(e.to_string())))?;
                Ok(Box::new(AccessTokenList(tokens)))
            })
        }

        pub fn get_product_info(
            &self,
            rt: &Runtime,
            tokens: &AccessTokenList,
        ) -> Result<Box<AppInfoList>, Box<FfiError>> {
            let client = self
                .0
                .as_ref()
                .ok_or_else(|| Box::new(FfiError("client consumed".into())))?
                .as_logged_in()
                .map_err(|e| Box::new(FfiError(e)))?;

            rt.0.block_on(async {
                let infos = client
                    .pics_get_product_info(&tokens.0)
                    .await
                    .map_err(|e| Box::new(FfiError(e.to_string())))?;
                Ok(Box::new(AppInfoList(infos)))
            })
        }

        pub fn get_depot_key(
            &self,
            rt: &Runtime,
            depot_id: &DepotId,
            app_id: &AppId,
        ) -> Result<Box<DepotKey>, Box<FfiError>> {
            let client = self
                .0
                .as_ref()
                .ok_or_else(|| Box::new(FfiError("client consumed".into())))?
                .as_logged_in()
                .map_err(|e| Box::new(FfiError(e)))?;

            rt.0.block_on(async {
                let key = client
                    .get_depot_decryption_key(depot_id.0, app_id.0)
                    .await
                    .map_err(|e| Box::new(FfiError(e.to_string())))?;
                Ok(Box::new(DepotKey(key)))
            })
        }

        pub fn get_cdn_servers(
            &self,
            rt: &Runtime,
            cell_id: &CellId,
        ) -> Result<Box<CdnServerList>, Box<FfiError>> {
            let client = self
                .0
                .as_ref()
                .ok_or_else(|| Box::new(FfiError("client consumed".into())))?
                .as_logged_in()
                .map_err(|e| Box::new(FfiError(e)))?;

            rt.0.block_on(async {
                let servers = client
                    .get_cdn_servers(cell_id.0, None)
                    .await
                    .map_err(|e| Box::new(FfiError(e.to_string())))?;
                Ok(Box::new(CdnServerList(servers)))
            })
        }

        pub fn get_manifest_request_code(
            &self,
            rt: &Runtime,
            app_id: &AppId,
            depot_id: &DepotId,
            manifest_id: &ManifestId,
            branch: &DiplomatStr,
        ) -> Result<u64, Box<FfiError>> {
            let client = self
                .0
                .as_ref()
                .ok_or_else(|| Box::new(FfiError("client consumed".into())))?
                .as_logged_in()
                .map_err(|e| Box::new(FfiError(e)))?;
            let branch =
                std::str::from_utf8(branch).map_err(|e| Box::new(FfiError(e.to_string())))?;

            rt.0.block_on(async {
                client
                    .get_manifest_request_code(
                        app_id.0,
                        depot_id.0,
                        manifest_id.0,
                        Some(branch),
                        None,
                    )
                    .await
                    .map_err(|e| Box::new(FfiError(e.to_string())))
                    // 0 = no request code; CDN path omits the code segment
                    .map(|code| code.unwrap_or(0))
            })
        }
    }

    #[diplomat::opaque]
    pub struct RsaPublicKey {
        modulus: String,
        exponent: String,
        timestamp: u64,
    }

    impl RsaPublicKey {
        pub fn modulus(&self, write: &mut DiplomatWrite) {
            let _ = write!(write, "{}", self.modulus);
        }

        pub fn exponent(&self, write: &mut DiplomatWrite) {
            let _ = write!(write, "{}", self.exponent);
        }

        pub fn timestamp(&self) -> u64 {
            self.timestamp
        }
    }

    #[diplomat::opaque]
    pub struct AuthSession(steam::auth::AuthSession);

    impl AuthSession {
        pub fn guard_type_count(&self) -> u32 {
            self.0.allowed_confirmations.len() as u32
        }

        pub fn guard_type_at(&self, index: u32) -> GuardType {
            self.0
                .allowed_confirmations
                .get(index as usize)
                .map(|g| match g {
                    steam::auth::GuardType::EmailCode => GuardType::EmailCode,
                    steam::auth::GuardType::DeviceCode => GuardType::DeviceCode,
                    steam::auth::GuardType::DeviceConfirmation => GuardType::DeviceConfirmation,
                    _ => GuardType::None,
                })
                .unwrap_or(GuardType::None)
        }
    }

    #[diplomat::opaque]
    pub struct AuthTokens(steam::auth::AuthTokens);

    impl AuthTokens {
        pub fn access_token(&self, write: &mut DiplomatWrite) {
            let _ = write!(write, "{}", self.0.access_token);
        }

        pub fn refresh_token(&self, write: &mut DiplomatWrite) {
            let _ = write!(write, "{}", self.0.refresh_token);
        }

        pub fn account_name(&self, write: &mut DiplomatWrite) {
            if let Some(ref name) = self.0.account_name {
                let _ = write!(write, "{name}");
            }
        }
    }

    #[diplomat::opaque]
    pub struct AccessTokenList(Vec<steam::apps::AccessToken>);

    impl AccessTokenList {
        pub fn len(&self) -> u32 {
            self.0.len() as u32
        }
    }

    #[diplomat::opaque]
    pub struct AppInfoList(Vec<steam::apps::AppInfo>);

    impl AppInfoList {
        pub fn len(&self) -> u32 {
            self.0.len() as u32
        }

        pub fn app_id_at(&self, index: u32) -> u32 {
            self.0
                .get(index as usize)
                .and_then(|i| i.app_id)
                .map(|id| id.0)
                .unwrap_or(0)
        }

        pub fn kv_data_at(&self, index: u32) -> Box<AppInfoKv> {
            let data = self.0.get(index as usize).and_then(|i| i.kv_data.clone());
            Box::new(AppInfoKv(data))
        }
    }

    #[diplomat::opaque]
    pub struct AppInfoKv(Option<Vec<u8>>);

    impl AppInfoKv {
        pub fn has_data(&self) -> bool {
            self.0.is_some()
        }

        pub fn raw_bytes(&self) -> &[u8] {
            match &self.0 {
                Some(data) => data.as_slice(),
                None => &[],
            }
        }
    }

    #[diplomat::opaque]
    pub struct DepotKey(steam::depot::DepotKey);

    #[diplomat::opaque]
    pub struct CdnServerList(Vec<steam::cdn::server::CdnServer>);

    impl CdnServerList {
        pub fn len(&self) -> u32 {
            self.0.len() as u32
        }

        pub fn host_at(&self, index: u32, write: &mut DiplomatWrite) {
            if let Some(server) = self.0.get(index as usize) {
                let _ = write!(write, "{}", server.host);
            }
        }
    }

    #[diplomat::opaque_mut]
    pub struct CdnClient(steam::cdn::CdnClient);

    impl CdnClient {
        pub fn new() -> Result<Box<CdnClient>, Box<FfiError>> {
            steam::cdn::CdnClient::new()
                .map(|c| Box::new(CdnClient(c)))
                .map_err(|e| Box::new(FfiError(e.to_string())))
        }

        pub fn with_lancache(&mut self) {
            // Re-create with lancache flag since we can't move out of &mut self
            // This is a limitation of the FFI layer
            self.0 = self.0.clone().with_lancache();
        }

        pub fn download_manifest(
            &self,
            rt: &Runtime,
            cdn_servers: &CdnServerList,
            server_index: u32,
            depot_id: &DepotId,
            manifest_id: &ManifestId,
            request_code: u64,
        ) -> Result<Box<DepotManifest>, Box<FfiError>> {
            let server = cdn_servers
                .0
                .get(server_index as usize)
                .ok_or_else(|| Box::new(FfiError("server index out of bounds".into())))?;

            rt.0.block_on(async {
                let bytes = self
                    .0
                    .download_manifest(server, depot_id.0, manifest_id.0, request_code, None)
                    .await
                    .map_err(|e| Box::new(FfiError(e.to_string())))?;

                let manifest = steam_client::manifest::extract_and_parse(&bytes)
                    .map_err(|e| Box::new(FfiError(e.to_string())))?;

                Ok(Box::new(DepotManifest(manifest)))
            })
        }

        pub fn download_chunk(
            &self,
            rt: &Runtime,
            cdn_servers: &CdnServerList,
            server_index: u32,
            depot_id: &DepotId,
            chunk_id: &[u8],
        ) -> Result<Box<ChunkData>, Box<FfiError>> {
            let server = cdn_servers
                .0
                .get(server_index as usize)
                .ok_or_else(|| Box::new(FfiError("server index out of bounds".into())))?;

            if chunk_id.len() != 20 {
                return Err(Box::new(FfiError("chunk_id must be 20 bytes".into())));
            }
            let mut id = [0u8; 20];
            id.copy_from_slice(chunk_id);
            let chunk_id = steam::depot::ChunkId(id);

            rt.0.block_on(async {
                let bytes = self
                    .0
                    .download_chunk(server, depot_id.0, &chunk_id, None)
                    .await
                    .map_err(|e| Box::new(FfiError(e.to_string())))?;

                Ok(Box::new(ChunkData(bytes.to_vec())))
            })
        }
    }

    #[diplomat::opaque]
    pub struct ChunkData(Vec<u8>);

    impl ChunkData {
        pub fn as_bytes(&self) -> &[u8] {
            &self.0
        }

        pub fn len(&self) -> u32 {
            self.0.len() as u32
        }
    }

    #[diplomat::opaque_mut]
    pub struct DepotManifest(steam::depot::manifest::DepotManifest);

    impl DepotManifest {
        pub fn parse(data: &[u8]) -> Result<Box<DepotManifest>, Box<FfiError>> {
            steam_client::manifest::extract_and_parse(data)
                .map(|m| Box::new(DepotManifest(m)))
                .map_err(|e| Box::new(FfiError(e.to_string())))
        }

        pub fn file_count(&self) -> u32 {
            self.0.files.len() as u32
        }

        pub fn filenames_encrypted(&self) -> bool {
            self.0.filenames_encrypted
        }

        pub fn total_uncompressed_size(&self) -> u64 {
            self.0.total_uncompressed_size.unwrap_or(0)
        }

        pub fn total_compressed_size(&self) -> u64 {
            self.0.total_compressed_size.unwrap_or(0)
        }

        pub fn creation_time(&self) -> u32 {
            self.0.creation_time.unwrap_or(0)
        }

        pub fn file_name(&self, index: u32, write: &mut DiplomatWrite) {
            if let Some(file) = self.0.files.get(index as usize)
                && let Some(ref name) = file.filename
            {
                let _ = write!(write, "{name}");
            }
        }

        pub fn file_size(&self, index: u32) -> u64 {
            self.0
                .files
                .get(index as usize)
                .and_then(|f| f.size)
                .unwrap_or(0)
        }

        pub fn file_chunk_count(&self, index: u32) -> u32 {
            self.0
                .files
                .get(index as usize)
                .map(|f| f.chunks.len() as u32)
                .unwrap_or(0)
        }

        pub fn decrypt_filenames(&mut self, key: &DepotKey) -> Result<(), Box<FfiError>> {
            self.0
                .decrypt_filenames(&key.0)
                .map_err(|e| Box::new(FfiError(e.to_string())))
        }
    }

    #[diplomat::opaque_mut]
    pub struct TokenStore(steam_client::credentials::TokenStore);

    impl TokenStore {
        pub fn load_default() -> Box<TokenStore> {
            let path = steam_client::credentials::TokenStore::default_path();
            Box::new(TokenStore(steam_client::credentials::TokenStore::load(
                &path,
            )))
        }

        pub fn get(&self, username: &DiplomatStr, write: &mut DiplomatWrite) -> bool {
            let username = match std::str::from_utf8(username) {
                Ok(s) => s,
                Err(_) => return false,
            };
            match self.0.get(username) {
                Some(token) => {
                    let _ = write!(write, "{token}");
                    true
                }
                None => false,
            }
        }

        pub fn set(&mut self, username: &DiplomatStr, token: &DiplomatStr) {
            let username = std::str::from_utf8(username).unwrap_or_default();
            let token = std::str::from_utf8(token).unwrap_or_default();
            self.0.set(username.to_string(), token.to_string());
        }

        pub fn save(&self) -> Result<(), Box<FfiError>> {
            let path = steam_client::credentials::TokenStore::default_path();
            self.0
                .save(&path)
                .map_err(|e| Box::new(FfiError(e.to_string())))
        }
    }

    pub fn detect_lancache(rt: &Runtime) -> bool {
        rt.0.block_on(steam::cdn::lancache::detect())
    }
}
