mod cli;
mod download;
mod errors;

use std::collections::HashMap;
use std::path::PathBuf;

use prost::Message;

use crate::cli::Action;
use crate::cli::Options;
use crate::cli::OutputFormat;
use crate::errors::CliError;
use steam::client::DisconnectedClient;
use steam::client::msg::ClientMsg;
use steam::connection::CmServer;
use steam::connection::default_cm_servers;
use steam::connection::fetch_cm_servers;
use steam::depot::AppId;
use steam::depot::BuildId;
use steam::depot::CellId;
use steam::depot::DepotId;
use steam::depot::ManifestId;
use steam::messages::EMsg;

fn fmt_size(bytes: u64, raw: bool) -> String {
    if raw {
        return bytes.to_string();
    }
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    const GIB: u64 = 1024 * MIB;
    if bytes >= GIB {
        format!("{:.2} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.2} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.2} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

fn fmt_timestamp(unix: u32) -> String {
    jiff::Timestamp::from_second(unix as i64)
        .map(|ts| ts.strftime("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|_| unix.to_string())
}

fn fmt_timestamp_u64(unix: u64) -> String {
    jiff::Timestamp::from_second(unix as i64)
        .map(|ts| ts.strftime("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|_| unix.to_string())
}

#[tokio::main]
async fn main() {
    let opts = match Options::parse() {
        Ok(opts) => opts,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    let filter = if opts.debug {
        "steam=debug,steam_client=debug,depotdownloader=debug"
    } else if cfg!(debug_assertions) {
        "steam=info,steam_client=info,depotdownloader=info"
    } else {
        "steam=warn,steam_client=warn,depotdownloader=warn"
    };
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let raw_errors = opts.raw_errors;
    let result = match opts.action {
        Action::Info(ref args) => run_info(&opts, args).await,
        Action::Manifests(ref args) => run_manifests(&opts, args).await,
        Action::Files(ref args) => run_files(&opts, args).await,
        Action::Download(ref args) => run_download(&opts, args).await,
        Action::Workshop(ref args) => run_workshop(&opts, args).await,
    };

    if let Err(e) = result {
        e.print(raw_errors);
        std::process::exit(1);
    }
}

async fn discover_servers(cell_id: CellId) -> Vec<CmServer> {
    let http = reqwest::Client::new();

    match fetch_cm_servers(&http, cell_id).await {
        Ok(servers) if !servers.is_empty() => {
            tracing::info!("Got {} CM servers from Steam Directory API", servers.len());
            let tcp: Vec<_> = servers
                .into_iter()
                .filter(|s| s.protocol == steam::connection::Protocol::Tcp)
                .collect();
            if !tcp.is_empty() {
                return tcp;
            }
        }
        Ok(_) => tracing::warn!("Steam Directory returned no servers, using defaults"),
        Err(e) => tracing::warn!("Steam Directory API failed: {e}, using defaults"),
    }

    default_cm_servers()
}

/// Connect, encrypt, and login. If `--capture` is set, wraps the transport
/// with a recording layer and flushes on success.
async fn connect_and_login(
    opts: &Options,
) -> Result<steam::client::SteamClient<steam::client::LoggedIn>, CliError> {
    // 0 = default cell, any geographic region
    let cell_id = CellId(opts.cell_id.unwrap_or(0));
    let servers = discover_servers(cell_id).await;

    if servers.is_empty() {
        return Err("No CM servers available".into());
    }

    let mut last_err = None;
    for server in &servers {
        tracing::info!("Connecting to {:?}...", server.addr);
        match try_connect_login(server, opts).await {
            Ok(client) => return Ok(client),
            Err(e) if e.is_auth_error() => return Err(e),
            Err(e) => {
                tracing::warn!("Failed to connect to {:?}: {e}", server.addr);
                last_err = Some(e);
            }
        }
    }

    Err(last_err.unwrap_or_else(|| "No CM servers available".into()))
}

async fn try_connect_login(
    server: &CmServer,
    opts: &Options,
) -> Result<steam::client::SteamClient<steam::client::LoggedIn>, CliError> {
    let (client, _events) = DisconnectedClient::new();

    let client = if let Some(capture_path) = &opts.capture {
        use steam::transport::recording::RecordingTransport;
        use steam::transport::tcp::TcpTransport;

        let tcp = TcpTransport::connect(server).await?;
        let desc = format!("capture from {:?}", server.addr);
        let recording = RecordingTransport::new(tcp, PathBuf::from(capture_path), desc);
        client.connect(recording)
    } else {
        client.connect_tcp(server).await?
    };

    do_login(opts, client).await
}

async fn do_login(
    opts: &Options,
    client: steam::client::SteamClient<steam::client::Connected>,
) -> Result<steam::client::SteamClient<steam::client::LoggedIn>, CliError> {
    tracing::info!("Connected, performing encryption handshake...");

    let client = client.encrypt().await?;
    tracing::info!("Encrypted");

    if let Some(ref username) = opts.auth.username {
        // Try stored token first
        let token_path = steam_client::credentials::TokenStore::default_path();
        let mut store = steam_client::credentials::TokenStore::load(&token_path);

        let access_token = if let Some(stored) = store.get(username) {
            tracing::info!("Using stored credentials for {username}");
            stored.to_string()
        } else {
            tracing::info!("Authenticating as {username}...");
            let token = authenticate_credentials(&client, username, opts).await?;

            if opts.auth.remember_password {
                store.set(username.clone(), token.clone());
                if let Err(e) = store.save(&token_path) {
                    tracing::warn!("Failed to save credentials: {e}");
                } else {
                    tracing::info!("Credentials saved for {username}");
                }
            }

            token
        };

        // Log on with the access token
        let logon = steam::generated::CMsgClientLogon {
            protocol_version: Some(steam::client::PROTOCOL_VERSION),
            cell_id: Some(opts.cell_id.unwrap_or(0)),
            client_os_type: Some(steam::enums::EOSType::Windows11 as u32),
            client_language: Some("english".to_string()),
            access_token: Some(access_token),
            account_name: Some(username.clone()),
            should_remember_password: Some(opts.auth.remember_password),
            ..Default::default()
        };
        let logon_body = logon.encode_to_vec();
        let mut logon_msg = ClientMsg::with_body(EMsg::CLIENT_LOGON, &logon_body);

        let user_id = steam::types::SteamId::from_parts(
            steam::enums::EUniverse::Public as u8,
            steam::enums::EAccountType::Individual as u8,
            0,
            0,
        );
        logon_msg.header.steamid = Some(user_id.raw());
        logon_msg.header.client_sessionid = Some(0);

        let (client, _logon_resp) = client.login(logon_msg).await?;
        tracing::info!("Logged in successfully as {username}");
        Ok(client)
    } else if opts.auth.qr {
        tracing::info!("Starting QR code login...");
        let access_token = authenticate_qr(&client, opts).await?;

        let logon = steam::generated::CMsgClientLogon {
            protocol_version: Some(steam::client::PROTOCOL_VERSION),
            cell_id: Some(opts.cell_id.unwrap_or(0)),
            client_os_type: Some(steam::enums::EOSType::Windows11 as u32),
            client_language: Some("english".to_string()),
            access_token: Some(access_token),
            ..Default::default()
        };
        let logon_body = logon.encode_to_vec();
        let mut logon_msg = ClientMsg::with_body(EMsg::CLIENT_LOGON, &logon_body);

        let user_id = steam::types::SteamId::from_parts(
            steam::enums::EUniverse::Public as u8,
            steam::enums::EAccountType::Individual as u8,
            0,
            0,
        );
        logon_msg.header.steamid = Some(user_id.raw());
        logon_msg.header.client_sessionid = Some(0);

        let (client, _logon_resp) = client.login(logon_msg).await?;
        tracing::info!("Logged in successfully via QR code");
        Ok(client)
    } else {
        tracing::info!("Logging in anonymously...");
        let logon_body = build_logon_body(opts);
        let mut logon_msg = ClientMsg::with_body(EMsg::CLIENT_LOGON, &logon_body);

        let anon_id = steam::types::SteamId::from_parts(
            steam::enums::EUniverse::Public as u8,
            steam::enums::EAccountType::AnonUser as u8,
            0,
            0,
        );
        logon_msg.header.steamid = Some(anon_id.raw());
        logon_msg.header.client_sessionid = Some(0);

        let (client, _logon_resp) = client.login(logon_msg).await?;
        tracing::info!("Logged in anonymously");
        Ok(client)
    }
}

/// QR code authentication (pre-logon, on Encrypted state).
async fn authenticate_qr(
    client: &steam::client::SteamClient<steam::client::Encrypted>,
    opts: &Options,
) -> Result<String, CliError> {
    let request = steam::generated::CAuthenticationBeginAuthSessionViaQrRequest {
        device_friendly_name: Some(opts.auth.device_name.clone()),
        platform_type: Some(steam::enums::EAuthTokenPlatformType::WebBrowser as i32),
        ..Default::default()
    };

    let session = client.begin_auth_session_via_qr(request).await?;

    let challenge_url = session
        .challenge_url
        .as_deref()
        .ok_or("no QR challenge URL")?;
    let client_id = session.client_id.ok_or("no client_id")?;
    let request_id = session.request_id.as_ref().ok_or("no request_id")?;

    // Render QR code to terminal
    let qr = qrcode::QrCode::new(challenge_url.as_bytes())
        .map_err(|e| format!("Failed to generate QR code: {e}"))?;
    let image = qr
        .render::<char>()
        .quiet_zone(false)
        .module_dimensions(2, 1)
        .build();
    eprintln!("\nScan this QR code with the Steam mobile app:\n");
    eprintln!("{image}");
    eprintln!("\nOr open: {challenge_url}\n");
    eprintln!("Waiting for confirmation...");

    loop {
        let interval = session.poll_interval.unwrap_or(5.0);
        tokio::time::sleep(std::time::Duration::from_secs_f32(interval)).await;

        match client.poll_auth_session(client_id, request_id).await? {
            Some(tokens) => {
                tracing::info!("QR authentication successful");
                return Ok(tokens.refresh_token);
            }
            None => {
                tracing::debug!("Waiting for QR scan...");
            }
        }
    }
}

/// Perform credential-based authentication (pre-logon, on Encrypted state).
async fn authenticate_credentials(
    client: &steam::client::SteamClient<steam::client::Encrypted>,
    username: &str,
    opts: &Options,
) -> Result<String, CliError> {
    use rsa::BoxedUint;
    use rsa::Pkcs1v15Encrypt;
    use rsa::RsaPublicKey;

    const MAX_PASSWORD_ATTEMPTS: u32 = 3;
    let mut attempts = 0;

    let session = loop {
        attempts += 1;

        let password = match &opts.auth.password {
            Some(p) if attempts == 1 => p.clone(),
            _ => {
                eprint!("Password for {username}: ");
                rpassword::read_password()
                    .map_err(|e| CliError::Other(format!("failed to read password: {e}")))?
            }
        };

        tracing::debug!("Password length: {} bytes", password.len());

        let rsa_response = client.get_password_rsa_public_key(username).await?;
        let modulus = rsa_response.publickey_mod.ok_or("missing RSA modulus")?;
        let exponent = rsa_response.publickey_exp.ok_or("missing RSA exponent")?;
        let timestamp = rsa_response.timestamp.ok_or("missing RSA timestamp")?;

        let n_bits = (modulus.len() as u32) * 4;
        let n: BoxedUint = BoxedUint::from_be_hex(&modulus, n_bits)
            .into_option()
            .ok_or("invalid RSA modulus hex")?;
        let e_bits = (exponent.len() as u32) * 4;
        let e: BoxedUint = BoxedUint::from_be_hex(&exponent, e_bits)
            .into_option()
            .ok_or("invalid RSA exponent hex")?;
        let public_key = RsaPublicKey::new(n, e).map_err(|e| format!("invalid RSA key: {e}"))?;

        let mut rng = rand::rng();
        let encrypted_password = public_key
            .encrypt(&mut rng, Pkcs1v15Encrypt, password.as_bytes())
            .map_err(|e| format!("RSA encryption failed: {e}"))?;

        let encrypted_password_b64 =
            base64::engine::general_purpose::STANDARD.encode(&encrypted_password);

        let begin_request =
            steam::generated::CAuthenticationBeginAuthSessionViaCredentialsRequest {
                account_name: Some(username.to_string()),
                encrypted_password: Some(encrypted_password_b64),
                encryption_timestamp: Some(timestamp),
                persistence: Some(steam::enums::ESessionPersistence::Persistent as i32),
                website_id: Some("Client".to_string()),
                device_details: Some(steam::generated::CAuthenticationDeviceDetails {
                    device_friendly_name: Some(opts.auth.device_name.clone()),
                    platform_type: Some(steam::enums::EAuthTokenPlatformType::SteamClient as i32),
                    os_type: Some(steam::enums::EOSType::Windows11 as i32),
                    ..Default::default()
                }),
                ..Default::default()
            };

        match client
            .begin_auth_session_via_credentials(begin_request)
            .await
        {
            Ok(session) => break session,
            Err(steam::error::Error::Connection(
                steam::error::ConnectionError::ServiceMethodFailed(
                    steam::enums::EResultError::InvalidPassword,
                ),
            )) if attempts < MAX_PASSWORD_ATTEMPTS => {
                eprintln!(
                    "Invalid password. Please try again ({attempts}/{MAX_PASSWORD_ATTEMPTS})."
                );
            }
            Err(e) => return Err(e.into()),
        }
    };

    let client_id = session
        .client_id
        .ok_or("missing client_id in auth session")?;
    let request_id = session.request_id.as_ref().ok_or("missing request_id")?;
    let steam_id = session.steam_id.ok_or("missing steam_id")?;

    // Step 4: Handle 2FA if needed
    for confirmation in &session.allowed_confirmations {
        match confirmation {
            steam::auth::GuardType::DeviceCode => {
                eprint!("Steam Guard code (authenticator app): ");
                let mut code = String::new();
                std::io::stdin().read_line(&mut code)?;
                client
                    .submit_steam_guard_code(
                        client_id,
                        steam_id,
                        code.trim(),
                        steam::auth::GuardType::DeviceCode,
                    )
                    .await?;
                break;
            }
            steam::auth::GuardType::EmailCode => {
                eprint!("Steam Guard code (email): ");
                let mut code = String::new();
                std::io::stdin().read_line(&mut code)?;
                client
                    .submit_steam_guard_code(
                        client_id,
                        steam_id,
                        code.trim(),
                        steam::auth::GuardType::EmailCode,
                    )
                    .await?;
                break;
            }
            steam::auth::GuardType::DeviceConfirmation => {
                eprintln!("Confirm login on your Steam mobile app...");
            }
            _ => {}
        }
    }

    // Step 5: Poll for auth completion
    loop {
        let interval = session.poll_interval.unwrap_or(5.0);
        tokio::time::sleep(std::time::Duration::from_secs_f32(interval)).await;

        match client.poll_auth_session(client_id, request_id).await? {
            Some(tokens) => {
                tracing::info!("Authentication successful");
                return Ok(tokens.refresh_token);
            }
            None => {
                tracing::debug!("Auth session pending, polling again...");
            }
        }
    }
}

use base64::Engine;

async fn run_info(opts: &Options, args: &cli::InfoArgs) -> Result<(), CliError> {
    let app_id = AppId(args.app);
    let client = connect_and_login(opts).await?;

    let app_infos = get_app_info(&client, &[app_id]).await?;
    let branches = discover_branches(&app_infos);
    let depots = discover_depot_details(&app_infos);

    #[derive(serde::Serialize)]
    struct AppOverview {
        app_id: AppId,
        branches: Vec<BranchOverview>,
        depots: Vec<DepotOverview>,
    }

    #[derive(serde::Serialize)]
    struct BranchOverview {
        name: String,
        build_id: Option<BuildId>,
        time_updated: Option<u64>,
        password_required: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        manifests: Vec<DepotManifestEntry>,
    }

    #[derive(serde::Serialize)]
    struct DepotOverview {
        id: DepotId,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        os_list: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        os_arch: Option<String>,
    }

    let overview = AppOverview {
        app_id,
        branches: branches
            .iter()
            .map(|b| BranchOverview {
                name: b.name.clone(),
                build_id: b.build_id,
                time_updated: b.time_updated,
                password_required: b.password_required,
                description: b.description.clone(),
                manifests: discover_manifests_for_branch(&app_infos, &b.name, None),
            })
            .collect(),
        depots: depots
            .iter()
            .map(|d| DepotOverview {
                id: d.id,
                name: d.name.clone(),
                os_list: d.os_list.clone(),
                os_arch: d.os_arch.clone(),
            })
            .collect(),
    };

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&overview)?);
        }
        OutputFormat::Plain => {
            for d in &overview.depots {
                println!("{}", d.id.0);
            }
        }
        OutputFormat::Table => {
            println!("App {app_id}");
            println!();

            println!("Branches:");
            println!("  {:<20} {:>10} {:>22} FLAGS", "NAME", "BUILD", "UPDATED");
            for b in &overview.branches {
                let flags = if b.password_required {
                    "[password]"
                } else {
                    ""
                };
                let updated = b.time_updated.map(fmt_timestamp_u64).unwrap_or_default();
                println!(
                    "  {:<20} {:>10} {:>22} {}",
                    b.name,
                    b.build_id.map(|id| id.to_string()).unwrap_or_default(),
                    updated,
                    flags,
                );
            }
            println!();

            println!("Depots:");
            println!("  {:<12} {:<30} {:<20} ARCH", "ID", "NAME", "OS");
            for d in &overview.depots {
                println!(
                    "  {:<12} {:<30} {:<20} {}",
                    d.id.0,
                    d.name.as_deref().unwrap_or(""),
                    d.os_list.as_deref().unwrap_or(""),
                    d.os_arch.as_deref().unwrap_or(""),
                );
            }
            println!();

            for b in &overview.branches {
                if !b.manifests.is_empty() {
                    println!("Manifests for branch '{}':", b.name);
                    for m in &b.manifests {
                        println!(
                            "  depot {:<10} → {}",
                            m.depot_id.0,
                            m.manifest_id
                                .map(|id| id.0.to_string())
                                .unwrap_or_else(|| "—".into()),
                        );
                    }
                }
            }
        }
    }

    Ok(())
}

async fn run_download(opts: &Options, args: &cli::DownloadArgs) -> Result<(), CliError> {
    let app_id = AppId(args.app);
    tracing::info!("Downloading app {app_id}");

    let client = connect_and_login(opts).await?;
    let cell_id = CellId(opts.cell_id.unwrap_or(0));

    let app_infos = get_app_info(&client, &[app_id]).await?;

    let cdn_servers = client.get_cdn_servers(cell_id, None).await?;
    if cdn_servers.is_empty() {
        return Err("No CDN servers available".into());
    }

    let filter = DepotFilter {
        os: args.os.as_deref(),
        arch: args.arch.as_deref(),
        language: args.language.as_deref(),
        all_platforms: args.all_platforms,
        all_archs: args.all_archs,
        all_languages: args.all_languages,
    };

    let depot_ids: Vec<DepotId> = if args.depot.is_empty() {
        discover_depots_filtered(&app_infos, &filter)
    } else {
        args.depot.iter().map(|&id| DepotId(id)).collect()
    };

    if depot_ids.is_empty() {
        return Err("No depots to download".into());
    }

    // Build manifest ID lookup: explicit IDs take priority, otherwise discover from PICS
    let explicit_manifests: HashMap<DepotId, ManifestId> = depot_ids
        .iter()
        .zip(args.manifest.iter())
        .map(|(&d, &m)| (d, ManifestId(m)))
        .collect();

    let custom_output = args.output.is_some();
    let base_dir = args
        .output
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("depots"));

    let build_id = discover_build_id(&app_infos, &args.branch);

    let mut cdn = steam::cdn::CdnClient::new()?;
    let mut max_downloads = opts.max_downloads;

    if args.lancache {
        if steam::cdn::lancache::detect().await {
            cdn = cdn.with_lancache();
            if max_downloads == 8 {
                max_downloads = 25;
            }
            tracing::info!("Lancache detected, using local cache (max_downloads={max_downloads})");
        } else {
            tracing::warn!("--lancache specified but no lancache server detected on the network");
        }
    }

    for &depot_id in &depot_ids {
        tracing::info!("Processing depot {depot_id}...");

        let depot_key = client.get_depot_decryption_key(depot_id, app_id).await?;

        let manifest_id = match explicit_manifests.get(&depot_id) {
            Some(&id) => id,
            None => {
                resolve_manifest_id(
                    &client,
                    &app_infos,
                    app_id,
                    depot_id,
                    &args.branch,
                    args.beta_password.as_deref(),
                )
                .await?
            }
        };

        tracing::info!("Depot {depot_id} manifest {manifest_id}");

        // 0 = no request code; CDN path omits the code segment
        let request_code = client
            .get_manifest_request_code(app_id, depot_id, manifest_id, Some(&args.branch), None)
            .await?
            .unwrap_or(0);

        let cdn_token = match client
            .get_cdn_auth_token(app_id, depot_id, &cdn_servers[0].host)
            .await
        {
            Ok(auth) => auth.token,
            Err(e) => {
                tracing::debug!("CDN auth token not available: {e}");
                None
            }
        };

        // Try cached manifest first
        let cache = steam_client::manifest::ManifestCache::default_for(&base_dir);
        let mut manifest = if let Some(cached) = cache.load(depot_id, manifest_id) {
            cached
        } else {
            let manifest_bytes = cdn
                .download_manifest(
                    &cdn_servers[0],
                    depot_id,
                    manifest_id,
                    request_code,
                    cdn_token.as_deref(),
                )
                .await?;

            if let Err(e) = cache.save(depot_id, manifest_id, &manifest_bytes) {
                tracing::warn!("Failed to cache manifest: {e}");
            }

            steam_client::manifest::extract_and_parse(&manifest_bytes)?
        };

        // Decrypt filenames if needed
        if manifest.filenames_encrypted {
            manifest.decrypt_filenames(&depot_key)?;
        }

        tracing::info!(
            "Manifest: {} files, {}",
            manifest.files.len(),
            manifest
                .total_uncompressed_size
                .map(|s| fmt_size(s, opts.raw_bytes))
                .unwrap_or_else(|| "unknown size".into()),
        );

        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let progress_handle = download::spawn_progress_renderer(event_rx);

        // Build file filter from CLI args
        let file_filter = if let Some(ref path) = args.filelist {
            Some(steam_client::download::FileFilter::from_filelist(
                std::path::Path::new(path),
            )?)
        } else if let Some(ref pattern) = args.file_regex {
            Some(
                steam_client::download::FileFilter::from_regex(pattern)
                    .map_err(|e| -> CliError { format!("Invalid regex: {e}").into() })?,
            )
        } else {
            None
        };

        // Load previous manifest for delta downloads
        let config_path = steam_client::manifest::DepotConfig::path_for(&base_dir);
        let depot_config = steam_client::manifest::DepotConfig::load(&config_path);
        let installed_id = depot_config.get_installed(depot_id);

        if installed_id == Some(manifest_id) && !args.verify {
            tracing::info!("Depot {depot_id} already up to date (manifest {manifest_id})");
            continue;
        }

        let previous_manifest = installed_id
            .filter(|&old_id| old_id != manifest_id)
            .and_then(|old_id| cache.load(depot_id, old_id));

        if previous_manifest.is_some() {
            tracing::info!("Delta download: comparing against previous manifest");
        }

        let job = steam_client::download::DepotJob::builder()
            .cdn(cdn.clone(), cdn_servers[0].clone(), cdn_token.clone())
            .depot_id(depot_id)
            .depot_key(depot_key)
            .install_dir(if custom_output {
                base_dir.clone()
            } else {
                let mut p = base_dir.join(format!("{depot_id}"));
                if let Some(bid) = build_id {
                    p = p.join(bid.to_string());
                }
                p
            })
            .max_downloads(max_downloads)
            .event_sender(event_tx)
            .verify(args.verify)
            .file_filter(file_filter)
            .previous_manifest(previous_manifest)
            .build()
            .map_err(|e| -> CliError { e.into() })?;

        job.download(&manifest).await?;
        drop(job); // Drop the event sender so the progress renderer sees channel close
        progress_handle.await?;

        // Track installed manifest for future delta downloads
        let config_path = steam_client::manifest::DepotConfig::path_for(&base_dir);
        let mut depot_config = steam_client::manifest::DepotConfig::load(&config_path);
        depot_config.set_installed(depot_id, manifest_id);
        if let Err(e) = depot_config.save(&config_path) {
            tracing::warn!("Failed to save depot config: {e}");
        }

        tracing::info!("Depot {depot_id} download complete");
    }

    tracing::info!("All done");
    Ok(())
}

async fn run_manifests(opts: &Options, args: &cli::ManifestsArgs) -> Result<(), CliError> {
    let app_id = AppId(args.app);
    let client = connect_and_login(opts).await?;

    let app_infos = get_app_info(&client, &[app_id]).await?;
    let depot_filter = args.depot.map(DepotId);
    let entries = discover_manifests_for_branch(&app_infos, &args.branch, depot_filter);

    if entries.is_empty() {
        return Err(format!("No manifests found on branch '{}'", args.branch).into());
    }

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&entries)?);
        }
        OutputFormat::Plain => {
            for e in &entries {
                if let Some(id) = e.manifest_id {
                    println!("{} {}", e.depot_id.0, id.0);
                }
            }
        }
        OutputFormat::Table => {
            println!("{:<12} {:<30} {:>22}", "DEPOT", "NAME", "MANIFEST ID");
            for e in &entries {
                println!(
                    "{:<12} {:<30} {:>22}",
                    e.depot_id.0,
                    e.depot_name.as_deref().unwrap_or(""),
                    e.manifest_id
                        .map(|id| id.0.to_string())
                        .unwrap_or_else(|| "—".into()),
                );
            }
        }
    }

    Ok(())
}

async fn run_files(opts: &Options, args: &cli::FilesArgs) -> Result<(), CliError> {
    let app_id = AppId(args.app);
    let depot_id = DepotId(args.depot);

    let client = connect_and_login(opts).await?;
    let cell_id = CellId(opts.cell_id.unwrap_or(0));

    let manifest_id = match args.manifest {
        Some(id) => ManifestId(id),
        None => {
            // Auto-discover manifest ID from branch via PICS
            let app_infos = get_app_info(&client, &[app_id]).await?;
            discover_manifest_id(&app_infos, depot_id, &args.branch).ok_or_else(
                || -> CliError {
                    format!(
                        "No manifest found for depot {depot_id} on branch '{}'",
                        args.branch
                    )
                    .into()
                },
            )?
        }
    };

    let cdn_servers = client.get_cdn_servers(cell_id, None).await?;
    if cdn_servers.is_empty() {
        return Err("No CDN servers available".into());
    }

    // 0 = no request code; CDN path omits the code segment
    let request_code = client
        .get_manifest_request_code(app_id, depot_id, manifest_id, Some(&args.branch), None)
        .await?
        .unwrap_or(0);

    let cdn = steam::cdn::CdnClient::new()?;
    let manifest_bytes = cdn
        .download_manifest(&cdn_servers[0], depot_id, manifest_id, request_code, None)
        .await?;

    let mut manifest = steam_client::manifest::extract_and_parse(&manifest_bytes)?;

    // Decrypt filenames unless --raw was requested
    if manifest.filenames_encrypted && !args.raw {
        match client.get_depot_decryption_key(depot_id, app_id).await {
            Ok(key) => {
                manifest.decrypt_filenames(&key)?;
            }
            Err(e) => {
                tracing::warn!("Could not get depot key for filename decryption: {e}");
                tracing::warn!("Use --raw to suppress this warning");
            }
        }
    }

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&manifest)?);
        }
        OutputFormat::Plain => {
            for file in &manifest.files {
                if let Some(name) = &file.filename {
                    println!("{name}");
                }
            }
        }
        OutputFormat::Table => {
            println!("Depot:    {depot_id}");
            println!("Manifest: {manifest_id}");
            if let Some(t) = manifest.creation_time {
                println!("Created:  {}", fmt_timestamp(t));
            }
            if let Some(size) = manifest.total_uncompressed_size {
                println!("Size:     {}", fmt_size(size, opts.raw_bytes));
            }
            println!("Files:    {}", manifest.files.len());
            if manifest.filenames_encrypted {
                println!("NOTE:     filenames are encrypted (use authenticated login or --raw)");
            }
            println!();
            println!("{:<60} {:>12} {:>8}", "FILENAME", "SIZE", "CHUNKS");
            for file in &manifest.files {
                let name = file.filename.as_deref().unwrap_or("<unnamed>");
                let size = file
                    .size
                    .map(|s| fmt_size(s, opts.raw_bytes))
                    .unwrap_or_else(|| "?".into());
                println!("{:<60} {:>12} {:>8}", name, size, file.chunks.len());
            }
        }
    }

    Ok(())
}

async fn run_workshop(opts: &Options, args: &cli::WorkshopArgs) -> Result<(), CliError> {
    let pubfile_id = match (args.pubfile, args.ugc) {
        (Some(id), _) => id,
        (_, Some(id)) => id,
        _ => return Err("Specify --pubfile or --ugc".into()),
    };

    let client = connect_and_login(opts).await?;

    // Query published file details
    let request = steam::generated::CPublishedFileGetDetailsRequest {
        publishedfileids: vec![pubfile_id],
        includechildren: Some(true),
        ..Default::default()
    };

    let encoded = prost::Message::encode_to_vec(&request);
    let resp = client
        .call_service_method("PublishedFile.GetDetails#1", &encoded)
        .await?;

    let details_resp: steam::generated::CPublishedFileGetDetailsResponse =
        prost::Message::decode(&resp.body[..])?;

    let details = details_resp
        .publishedfiledetails
        .first()
        .ok_or("No published file details returned")?;

    let title = details.title.as_deref().unwrap_or("Unknown");
    let app_id = details
        .consumer_appid
        .ok_or("Workshop item has no associated app ID")?;
    tracing::info!("Workshop item: {title} (app {app_id})");

    let output = args
        .output
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("workshop"));

    // If there's a direct file URL, download via HTTP
    if let Some(ref url) = details.file_url
        && !url.is_empty()
    {
        let filename = details.filename.as_deref().unwrap_or("workshop_item");

        tracing::info!("Downloading {filename} from {url}");
        let http = reqwest::Client::new();
        let resp = http.get(url).send().await?.error_for_status()?;
        let bytes = resp.bytes().await?;

        let out_path = output.join(filename);
        if let Some(parent) = out_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&out_path, &bytes).await?;
        tracing::info!("Saved to {}", out_path.display());
        return Ok(());
    }

    // Otherwise, download via depot manifest using hcontent_file
    if let Some(hcontent) = details.hcontent_file
        && hcontent > 0
    {
        let manifest_id = ManifestId(hcontent);
        let depot_id = DepotId(app_id);
        let cell_id = CellId(opts.cell_id.unwrap_or(0));

        tracing::info!("Downloading via depot manifest (manifest={manifest_id})");

        let cdn_servers = client.get_cdn_servers(cell_id, None).await?;
        if cdn_servers.is_empty() {
            return Err("No CDN servers available".into());
        }

        let depot_key = client
            .get_depot_decryption_key(depot_id, AppId(app_id))
            .await?;

        // 0 = no request code; CDN path omits the code segment
        let request_code = client
            .get_manifest_request_code(AppId(app_id), depot_id, manifest_id, None, None)
            .await?
            .unwrap_or(0);

        let cdn = steam::cdn::CdnClient::new()?;
        let manifest_bytes = cdn
            .download_manifest(&cdn_servers[0], depot_id, manifest_id, request_code, None)
            .await?;

        let mut manifest = steam_client::manifest::extract_and_parse(&manifest_bytes)?;
        if manifest.filenames_encrypted {
            let _ = manifest.decrypt_filenames(&depot_key);
        }

        tracing::info!("Workshop manifest: {} files", manifest.files.len());

        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let progress_handle = download::spawn_progress_renderer(event_rx);

        let job = steam_client::download::DepotJob::builder()
            .cdn(cdn, cdn_servers[0].clone(), None)
            .depot_id(depot_id)
            .depot_key(depot_key)
            .install_dir(output)
            .max_downloads(opts.max_downloads)
            .event_sender(event_tx)
            .build()
            .map_err(|e| -> CliError { e.into() })?;

        job.download(&manifest).await?;
        drop(job);
        progress_handle.await?;

        tracing::info!("Workshop download complete");
        return Ok(());
    }

    Err("Workshop item has no downloadable content".into())
}

/// Get product info for an app, handling missing access tokens.
async fn get_app_info(
    client: &steam::client::SteamClient<steam::client::LoggedIn>,
    app_ids: &[AppId],
) -> Result<Vec<steam::apps::AppInfo>, CliError> {
    let tokens = client.pics_get_access_tokens(app_ids).await?;
    tracing::debug!("Got {} PICS access token(s)", tokens.len());

    let query: Vec<steam::apps::AccessToken> = app_ids
        .iter()
        .map(|&app_id| {
            // 0 = no access token; free apps don't require one
            let token = tokens
                .iter()
                .find(|t| t.app_id == app_id)
                .map(|t| t.token)
                .unwrap_or(0);
            steam::apps::AccessToken { app_id, token }
        })
        .collect();

    let infos = client.pics_get_product_info(&query).await?;
    tracing::debug!("Got product info for {} app(s)", infos.len());
    Ok(infos)
}

fn build_logon_body(opts: &Options) -> Vec<u8> {
    let logon = steam::generated::CMsgClientLogon {
        protocol_version: Some(steam::client::PROTOCOL_VERSION),
        cell_id: Some(opts.cell_id.unwrap_or(0)),
        client_os_type: Some(steam::enums::EOSType::Windows11 as u32),
        client_language: Some("english".to_string()),
        account_name: opts.auth.username.clone(),
        ..Default::default()
    };
    logon.encode_to_vec()
}

/// Parse KV data from a PICS app info response (text or binary format).
fn parse_app_kv(info: &steam::apps::AppInfo) -> Option<steam::types::key_value::KeyValue> {
    use steam::types::key_value::parse_binary_kv;
    use steam::types::key_value::parse_text_kv;

    let kv_data = info.kv_data.as_ref()?;

    if let Ok(text) = std::str::from_utf8(kv_data) {
        match parse_text_kv(text) {
            Ok(kv) => return Some(kv),
            Err(e) => tracing::debug!("Failed to parse text KV: {e}"),
        }
    }

    let mut input = kv_data.as_slice();
    match parse_binary_kv(&mut input) {
        Ok(kv) => Some(kv),
        Err(e) => {
            tracing::debug!("Failed to parse binary KV: {e}");
            None
        }
    }
}

#[derive(Debug, serde::Serialize)]
struct DepotInfo {
    id: DepotId,
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    os_list: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    os_arch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<String>,
    /// If set, this depot's manifests come from another app.
    #[serde(skip_serializing_if = "Option::is_none")]
    depot_from_app: Option<u32>,
}

/// Depot filter criteria from CLI args.
struct DepotFilter<'a> {
    os: Option<&'a str>,
    arch: Option<&'a str>,
    language: Option<&'a str>,
    all_platforms: bool,
    all_archs: bool,
    all_languages: bool,
}

impl DepotFilter<'_> {
    fn matches(&self, depot: &DepotInfo) -> bool {
        // OS filter
        if !self.all_platforms
            && let (Some(filter_os), Some(depot_os)) = (self.os, depot.os_list.as_deref())
        {
            let os_list: Vec<&str> = depot_os.split(',').map(|s| s.trim()).collect();
            if !os_list.iter().any(|o| o.eq_ignore_ascii_case(filter_os)) {
                tracing::debug!(
                    "Depot {} skipped: OS {depot_os} doesn't match {filter_os}",
                    depot.id
                );
                return false;
            }
        }

        // Architecture filter
        if !self.all_archs
            && let (Some(filter_arch), Some(depot_arch)) = (self.arch, depot.os_arch.as_deref())
            && !depot_arch.eq_ignore_ascii_case(filter_arch)
        {
            tracing::debug!(
                "Depot {} skipped: arch {depot_arch} doesn't match {filter_arch}",
                depot.id
            );
            return false;
        }

        // Language filter
        if !self.all_languages
            && let (Some(filter_lang), Some(depot_lang)) =
                (self.language, depot.language.as_deref())
            && !depot_lang.eq_ignore_ascii_case(filter_lang)
        {
            tracing::debug!(
                "Depot {} skipped: language {depot_lang} doesn't match {filter_lang}",
                depot.id
            );
            return false;
        }

        true
    }
}

fn discover_depots_filtered(
    app_infos: &[steam::apps::AppInfo],
    filter: &DepotFilter<'_>,
) -> Vec<DepotId> {
    discover_depot_details(app_infos)
        .into_iter()
        .filter(|d| filter.matches(d))
        .map(|d| d.id)
        .collect()
}

fn discover_depot_details(app_infos: &[steam::apps::AppInfo]) -> Vec<DepotInfo> {
    use steam::types::key_value::KvValue;

    let mut depots = Vec::new();

    for info in app_infos {
        let kv = match parse_app_kv(info) {
            Some(kv) => kv,
            None => continue,
        };

        let depots_section = match kv.get("depots") {
            Some(d) => d,
            None => continue,
        };

        if let KvValue::Children(children) = &depots_section.value {
            for (key, value) in children {
                if let Ok(id) = key.parse::<u32>() {
                    let str_field =
                        |key: &str| value.get(key).and_then(|n| n.as_str()).map(String::from);
                    let depot_from_app = value
                        .get("depotfromapp")
                        .and_then(|n| n.as_str())
                        .and_then(|s| s.parse::<u32>().ok());
                    depots.push(DepotInfo {
                        id: DepotId(id),
                        name: str_field("name"),
                        os_list: str_field("oslist"),
                        os_arch: str_field("osarch"),
                        language: str_field("language"),
                        depot_from_app,
                    });
                }
            }
        }
    }

    depots
}

/// Branch metadata from PICS KV data.
#[derive(Debug, serde::Serialize)]
struct BranchInfo {
    name: String,
    build_id: Option<BuildId>,
    time_updated: Option<u64>,
    password_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

/// A depot's manifest on a specific branch.
#[derive(Debug, serde::Serialize)]
struct DepotManifestEntry {
    depot_id: DepotId,
    #[serde(skip_serializing_if = "Option::is_none")]
    depot_name: Option<String>,
    manifest_id: Option<ManifestId>,
    /// True if this manifest ID came from the `encryptedmanifests` section
    /// (requires a branch password to access).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    encrypted: bool,
}

/// Discover branches from the `depots → branches` section of PICS KV.
fn discover_branches(app_infos: &[steam::apps::AppInfo]) -> Vec<BranchInfo> {
    use steam::types::key_value::KvValue;

    let mut branches = Vec::new();

    for info in app_infos {
        let kv = match parse_app_kv(info) {
            Some(kv) => kv,
            None => continue,
        };

        let branches_section = kv.get("depots").and_then(|d| d.get("branches"));

        let branches_section = match branches_section {
            Some(b) => b,
            None => continue,
        };

        if let KvValue::Children(children) = &branches_section.value {
            for (name, branch_kv) in children {
                let str_field = |key: &str| branch_kv.get(key).and_then(|n| n.as_str());
                let u32_field = |key: &str| str_field(key).and_then(|s| s.parse::<u32>().ok());
                let u64_field = |key: &str| str_field(key).and_then(|s| s.parse::<u64>().ok());

                branches.push(BranchInfo {
                    name: name.clone(),
                    build_id: u32_field("buildid").map(BuildId),
                    time_updated: u64_field("timeupdated"),
                    password_required: str_field("pwdrequired")
                        .is_some_and(|v| v == "1" || v == "true"),
                    description: str_field("description").map(String::from),
                });
            }
        }
    }

    branches
}

/// List all depot manifests for a specific branch.
fn discover_manifests_for_branch(
    app_infos: &[steam::apps::AppInfo],
    branch: &str,
    depot_filter: Option<DepotId>,
) -> Vec<DepotManifestEntry> {
    use steam::types::key_value::KvValue;

    let mut entries = Vec::new();

    for info in app_infos {
        let kv = match parse_app_kv(info) {
            Some(kv) => kv,
            None => continue,
        };

        let depots_section = match kv.get("depots") {
            Some(d) => d,
            None => continue,
        };

        if let KvValue::Children(children) = &depots_section.value {
            for (key, depot_kv) in children {
                let depot_id = match key.parse::<u32>() {
                    Ok(id) => DepotId(id),
                    Err(_) => continue, // skip non-numeric keys like "branches"
                };

                if let Some(filter) = depot_filter
                    && depot_id != filter
                {
                    continue;
                }

                let depot_name = depot_kv
                    .get("name")
                    .and_then(|n| n.as_str())
                    .map(String::from);

                // Check public manifests first, then encrypted manifests
                let manifest_id = depot_kv
                    .get("manifests")
                    .and_then(|m| m.get(branch))
                    .and_then(|b| b.get("gid"))
                    .and_then(|g| g.as_str())
                    .and_then(|s| s.parse::<u64>().ok())
                    .map(ManifestId);

                let encrypted_manifest_id = if manifest_id.is_none() {
                    depot_kv
                        .get("encryptedmanifests")
                        .and_then(|m| m.get(branch))
                        .and_then(|b| b.get("gid"))
                        .and_then(|g| g.as_str())
                        .and_then(|s| s.parse::<u64>().ok())
                        .map(ManifestId)
                } else {
                    None
                };

                let (resolved_id, encrypted) = match (manifest_id, encrypted_manifest_id) {
                    (Some(id), _) => (Some(id), false),
                    (None, Some(id)) => (Some(id), true),
                    (None, None) => (None, false),
                };

                entries.push(DepotManifestEntry {
                    depot_id,
                    depot_name,
                    manifest_id: resolved_id,
                    encrypted,
                });
            }
        }
    }

    entries
}

/// Legacy helper: discover manifest ID for a specific depot on a branch.
fn discover_build_id(app_infos: &[steam::apps::AppInfo], branch: &str) -> Option<BuildId> {
    discover_branches(app_infos)
        .into_iter()
        .find(|b| b.name == branch)
        .and_then(|b| b.build_id)
}

fn discover_manifest_id(
    app_infos: &[steam::apps::AppInfo],
    depot_id: DepotId,
    branch: &str,
) -> Option<ManifestId> {
    discover_manifests_for_branch(app_infos, branch, Some(depot_id))
        .into_iter()
        .find_map(|e| e.manifest_id)
}

/// Resolve a manifest ID for a depot, trying:
/// 1. Public manifests for the branch
/// 2. Beta password if provided
/// 3. Fallback to "public" branch
async fn resolve_manifest_id(
    client: &steam::client::SteamClient<steam::client::LoggedIn>,
    app_infos: &[steam::apps::AppInfo],
    app_id: AppId,
    depot_id: DepotId,
    branch: &str,
    beta_password: Option<&str>,
) -> Result<ManifestId, CliError> {
    if let Some(id) = discover_manifest_id(app_infos, depot_id, branch) {
        return Ok(id);
    }

    // Check if this branch exists in encryptedmanifests and we have a password
    if let Some(password) = beta_password {
        tracing::info!("Branch '{branch}' requires a password, checking...");
        match client.check_beta_password(app_id, password).await {
            Ok(branches) => {
                for b in &branches {
                    tracing::debug!("Unlocked beta branch: {:?}", b.name);
                }
                // Re-fetch app info now that we've validated the password,
                // then try to find the manifest using the decrypted branch data.
                // The encrypted manifest GID should now be accessible.
                let entries = discover_manifests_for_branch(app_infos, branch, Some(depot_id));
                if let Some(entry) = entries.into_iter().find(|e| e.manifest_id.is_some()) {
                    return Ok(entry.manifest_id.unwrap());
                }
            }
            Err(e) => {
                tracing::warn!("Beta password check failed: {e}");
            }
        }
    }

    // Fallback to public branch
    if branch != "public" {
        tracing::warn!("Branch '{branch}' not found for depot {depot_id}, trying 'public'");
        if let Some(id) = discover_manifest_id(app_infos, depot_id, "public") {
            return Ok(id);
        }
    }

    Err(format!("No manifest for depot {depot_id} on branch '{branch}'").into())
}
