mod cli;
mod download;

use std::collections::HashMap;
use std::path::PathBuf;

use prost::Message;

use steam::client::DisconnectedClient;
use steam::client::msg::ClientMsg;
use steam::connection::{CmServer, fetch_cm_servers, default_cm_servers};
use steam::depot::{AppId, CellId, DepotId, ManifestId};
use steam::messages::EMsg;
use crate::cli::{Action, Options, OutputFormat};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    let opts = Options::parse();

    let filter = if opts.debug {
        "steam=debug,steam_client=debug,depotdownloader=debug"
    } else if cfg!(debug_assertions) {
        "steam=info,steam_client=info,depotdownloader=info"
    } else {
        "steam=warn,steam_client=warn,depotdownloader=warn"
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .init();

    match opts.action {
        Action::Download(ref args) => run_download(&opts, args).await,
        Action::Depots(ref args) => run_depots(&opts, args).await,
        Action::Manifests(ref args) => run_manifests(&opts, args).await,
        Action::Files(ref args) => run_files(&opts, args).await,
        Action::Workshop(ref args) => run_workshop(&opts, args).await,
    }
}

// ── Shared connection helper ─────────────────────────────────

async fn discover_servers(cell_id: u32) -> Vec<CmServer> {
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
) -> Result<steam::client::SteamClient<steam::client::LoggedIn>, BoxError> {
    let cell_id = opts.cell_id.unwrap_or(0);
    let servers = discover_servers(cell_id).await;

    if servers.is_empty() {
        return Err("No CM servers available".into());
    }

    let mut last_err = None;
    for server in &servers {
        tracing::info!("Connecting to {:?}...", server.addr);
        match try_connect_login(server, opts).await {
            Ok(client) => return Ok(client),
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
) -> Result<steam::client::SteamClient<steam::client::LoggedIn>, BoxError> {
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
) -> Result<steam::client::SteamClient<steam::client::LoggedIn>, BoxError> {
    tracing::info!("Connected, performing encryption handshake...");

    let client = client.encrypt().await?;
    tracing::info!("Encrypted");

    let logon_body = build_logon_body(opts);
    let mut logon_msg = ClientMsg::with_body(EMsg::CLIENT_LOGON, &logon_body);

    if opts.auth.username.is_none() {
        tracing::info!("Logging in anonymously...");
        let anon_id = steam::types::SteamId::from_parts(1, 10, 0, 0);
        logon_msg.header.steamid = Some(anon_id.raw());
        logon_msg.header.client_sessionid = Some(0);
    } else {
        tracing::info!("Logging in as {}...", opts.auth.username.as_deref().unwrap());
    }

    let (client, _logon_resp) = client.login(logon_msg).await?;
    tracing::info!("Logged in successfully");

    Ok(client)
}

// ── download ─────────────────────────────────────────────────

async fn run_download(opts: &Options, args: &cli::DownloadArgs) -> Result<(), BoxError> {
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

    let install_dir = args
        .output
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("depots"));

    let cdn = steam::cdn::CdnClient::new()?;

    for &depot_id in &depot_ids {
        tracing::info!("Processing depot {depot_id}...");

        let depot_key = client.get_depot_decryption_key(depot_id, app_id).await?;

        let manifest_id = match explicit_manifests.get(&depot_id) {
            Some(&id) => id,
            None => {
                // Auto-discover from PICS, with fallback to "public" branch
                let manifests = discover_manifests(&app_infos, depot_id);
                let found = manifests
                    .iter()
                    .find(|m| m.branch == args.branch)
                    .and_then(|m| m.manifest_id);

                match found {
                    Some(id) => id,
                    None if args.branch != "public" => {
                        tracing::warn!(
                            "Branch '{}' not found for depot {depot_id}, trying 'public'",
                            args.branch
                        );
                        manifests
                            .iter()
                            .find(|m| m.branch == "public")
                            .and_then(|m| m.manifest_id)
                            .ok_or_else(|| -> BoxError {
                                format!("No manifest for depot {depot_id} on any branch").into()
                            })?
                    }
                    None => {
                        return Err(format!(
                            "No manifest for depot {depot_id} on branch '{}'",
                            args.branch
                        ).into());
                    }
                }
            }
        };

        tracing::info!("Depot {depot_id} manifest {manifest_id}");

        let request_code = client
            .get_manifest_request_code(app_id, depot_id, manifest_id, Some(&args.branch), None)
            .await?
            .unwrap_or(0);

        let cdn_auth = client
            .get_cdn_auth_token(app_id, depot_id, &cdn_servers[0].host)
            .await?;

        let manifest_bytes = cdn
            .download_manifest(
                &cdn_servers[0], depot_id, manifest_id, request_code,
                cdn_auth.token.as_deref(),
            )
            .await?;

        let mut manifest = steam_client::manifest::extract_and_parse(&manifest_bytes)?;

        // Decrypt filenames if needed
        if manifest.filenames_encrypted {
            manifest.decrypt_filenames(&depot_key)?;
        }

        tracing::info!(
            "Manifest: {} files, {} bytes",
            manifest.files.len(),
            manifest.total_uncompressed_size.unwrap_or(0),
        );

        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let progress_handle = download::spawn_progress_renderer(event_rx);

        let job = steam_client::download::DepotJob::builder()
            .cdn(cdn.clone())
            .server(cdn_servers[0].clone())
            .depot_id(depot_id)
            .depot_key(depot_key)
            .cdn_auth_token(cdn_auth.token)
            .install_dir(install_dir.join(format!("{depot_id}")))
            .max_downloads(opts.max_downloads)
            .event_sender(event_tx)
            .verify(args.verify)
            .build()
            .map_err(|e| -> BoxError { e.into() })?;

        job.download(&manifest).await?;
        drop(job); // Drop the event sender so the progress renderer sees channel close
        progress_handle.await?;

        tracing::info!("Depot {depot_id} download complete");
    }

    tracing::info!("All done");
    Ok(())
}

// ── depots ───────────────────────────────────────────────────

async fn run_depots(opts: &Options, args: &cli::DepotsArgs) -> Result<(), BoxError> {
    let app_id = AppId(args.app);
    let client = connect_and_login(opts).await?;

    let app_infos = get_app_info(&client, &[app_id]).await?;
    let depots = discover_depot_details(&app_infos);

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&depots)?);
        }
        OutputFormat::Table => {
            println!("{:<12} {}", "DEPOT ID", "NAME");
            for depot in &depots {
                println!("{:<12} {}", depot.id.0, depot.name.as_deref().unwrap_or(""));
            }
        }
    }

    Ok(())
}

// ── manifests ────────────────────────────────────────────────

async fn run_manifests(opts: &Options, args: &cli::ManifestsArgs) -> Result<(), BoxError> {
    let app_id = AppId(args.app);
    let depot_id = DepotId(args.depot);
    let client = connect_and_login(opts).await?;

    let app_infos = get_app_info(&client, &[app_id]).await?;
    let manifests = discover_manifests(&app_infos, depot_id);

    if manifests.is_empty() {
        return Err(format!("No manifests found for depot {depot_id}").into());
    }

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&manifests)?);
        }
        OutputFormat::Table => {
            println!("{:<20} {:>20}", "BRANCH", "MANIFEST ID");
            for m in &manifests {
                println!(
                    "{:<20} {:>20}",
                    m.branch,
                    m.manifest_id.map(|id| id.0.to_string()).unwrap_or_else(|| "?".into()),
                );
            }
        }
    }

    Ok(())
}

// ── files ────────────────────────────────────────────────────

async fn run_files(opts: &Options, args: &cli::FilesArgs) -> Result<(), BoxError> {
    let app_id = AppId(args.app);
    let depot_id = DepotId(args.depot);

    let client = connect_and_login(opts).await?;
    let cell_id = CellId(opts.cell_id.unwrap_or(0));

    let manifest_id = match args.manifest {
        Some(id) => ManifestId(id),
        None => {
            // Auto-discover manifest ID from branch via PICS
            let app_infos = get_app_info(&client, &[app_id]).await?;
            let manifests = discover_manifests(&app_infos, depot_id);
            manifests
                .iter()
                .find(|m| m.branch == args.branch)
                .and_then(|m| m.manifest_id)
                .ok_or_else(|| -> BoxError {
                    format!(
                        "No manifest found for depot {depot_id} on branch '{}'",
                        args.branch
                    ).into()
                })?
        }
    };

    let cdn_servers = client.get_cdn_servers(cell_id, None).await?;
    if cdn_servers.is_empty() {
        return Err("No CDN servers available".into());
    }

    let request_code = client
        .get_manifest_request_code(app_id, depot_id, manifest_id, Some(&args.branch), None)
        .await?
        .unwrap_or(0);

    let cdn_auth = client
        .get_cdn_auth_token(app_id, depot_id, &cdn_servers[0].host)
        .await?;

    let cdn = steam::cdn::CdnClient::new()?;
    let manifest_bytes = cdn
        .download_manifest(
            &cdn_servers[0], depot_id, manifest_id, request_code,
            cdn_auth.token.as_deref(),
        )
        .await?;

    let mut manifest = steam_client::manifest::extract_and_parse(&manifest_bytes)?;

    // Decrypt filenames if encrypted
    if manifest.filenames_encrypted {
        match client.get_depot_decryption_key(depot_id, app_id).await {
            Ok(key) => {
                manifest.decrypt_filenames(&key)?;
                tracing::info!("Decrypted {} filenames", manifest.files.len());
            }
            Err(e) => {
                tracing::warn!("Could not get depot key for filename decryption: {e}");
                tracing::warn!("Filenames will be shown in encrypted form");
            }
        }
    }

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&manifest)?);
        }
        OutputFormat::Table => {
            println!("Depot:    {depot_id}");
            println!("Manifest: {manifest_id}");
            if let Some(t) = manifest.creation_time {
                println!("Created:  {t}");
            }
            if let Some(size) = manifest.total_uncompressed_size {
                println!("Size:     {size} bytes");
            }
            println!("Files:    {}", manifest.files.len());
            println!();
            println!("{:<60} {:>12} {:>8}", "FILENAME", "SIZE", "CHUNKS");
            for file in &manifest.files {
                let name = file.filename.as_deref().unwrap_or("<unnamed>");
                let size = file
                    .size
                    .map(|s| format!("{s}"))
                    .unwrap_or_else(|| "?".into());
                println!("{:<60} {:>12} {:>8}", name, size, file.chunks.len());
            }
        }
    }

    Ok(())
}

// ── workshop ─────────────────────────────────────────────────

async fn run_workshop(_opts: &Options, args: &cli::WorkshopArgs) -> Result<(), BoxError> {
    if let Some(id) = args.pubfile {
        tracing::info!("Workshop pubfile download not yet implemented (id: {id})");
    } else if let Some(id) = args.ugc {
        tracing::info!("Workshop UGC download not yet implemented (id: {id})");
    } else {
        return Err("Specify --pubfile or --ugc".into());
    }
    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────

/// Get product info for an app, handling missing access tokens.
async fn get_app_info(
    client: &steam::client::SteamClient<steam::client::LoggedIn>,
    app_ids: &[AppId],
) -> Result<Vec<steam::apps::AppInfo>, BoxError> {
    let tokens = client.pics_get_access_tokens(app_ids).await?;
    tracing::debug!("Got {} PICS access token(s)", tokens.len());

    let query: Vec<steam::apps::AccessToken> = app_ids
        .iter()
        .map(|&app_id| {
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
        protocol_version: Some(65581),
        cell_id: Some(opts.cell_id.unwrap_or(0)),
        client_os_type: Some(203),
        client_language: Some("english".to_string()),
        account_name: opts.auth.username.clone(),
        ..Default::default()
    };
    logon.encode_to_vec()
}

/// Parse KV data from a PICS app info response (text or binary format).
fn parse_app_kv(info: &steam::apps::AppInfo) -> Option<steam::types::key_value::KeyValue> {
    use steam::types::key_value::{parse_binary_kv, parse_text_kv};

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
        if !self.all_platforms {
            if let (Some(filter_os), Some(depot_os)) = (self.os, depot.os_list.as_deref()) {
                let os_list: Vec<&str> = depot_os.split(',').map(|s| s.trim()).collect();
                if !os_list.iter().any(|o| o.eq_ignore_ascii_case(filter_os)) {
                    tracing::debug!("Depot {} skipped: OS {depot_os} doesn't match {filter_os}", depot.id);
                    return false;
                }
            }
        }

        // Architecture filter
        if !self.all_archs {
            if let (Some(filter_arch), Some(depot_arch)) = (self.arch, depot.os_arch.as_deref()) {
                if !depot_arch.eq_ignore_ascii_case(filter_arch) {
                    tracing::debug!("Depot {} skipped: arch {depot_arch} doesn't match {filter_arch}", depot.id);
                    return false;
                }
            }
        }

        // Language filter
        if !self.all_languages {
            if let (Some(filter_lang), Some(depot_lang)) = (self.language, depot.language.as_deref()) {
                if !depot_lang.eq_ignore_ascii_case(filter_lang) {
                    tracing::debug!("Depot {} skipped: language {depot_lang} doesn't match {filter_lang}", depot.id);
                    return false;
                }
            }
        }

        true
    }
}

fn discover_depots_filtered(app_infos: &[steam::apps::AppInfo], filter: &DepotFilter<'_>) -> Vec<DepotId> {
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
                    let str_field = |key: &str| value.get(key).and_then(|n| n.as_str()).map(String::from);
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

#[derive(Debug, serde::Serialize)]
struct BranchManifest {
    branch: String,
    manifest_id: Option<ManifestId>,
}

fn discover_manifests(app_infos: &[steam::apps::AppInfo], depot_id: DepotId) -> Vec<BranchManifest> {
    use steam::types::key_value::KvValue;

    let mut manifests = Vec::new();
    let depot_key = depot_id.0.to_string();

    for info in app_infos {
        let kv = match parse_app_kv(info) {
            Some(kv) => kv,
            None => continue,
        };

        let depot = kv
            .get("depots")
            .and_then(|d| d.get(&depot_key));

        let depot = match depot {
            Some(d) => d,
            None => continue,
        };

        let manifests_section = match depot.get("manifests") {
            Some(m) => m,
            None => continue,
        };

        if let KvValue::Children(branches) = &manifests_section.value {
            for (branch_name, branch_kv) in branches {
                let gid_str = branch_kv
                    .get("gid")
                    .and_then(|g| g.as_str());

                let gid = match gid_str {
                    Some(s) => match s.parse::<u64>() {
                        Ok(id) => Some(ManifestId(id)),
                        Err(e) => {
                            tracing::warn!("Branch {branch_name}: failed to parse manifest ID {s:?}: {e}");
                            None
                        }
                    },
                    None => {
                        tracing::debug!("Branch {branch_name}: no 'gid' field");
                        None
                    }
                };

                manifests.push(BranchManifest {
                    branch: branch_name.clone(),
                    manifest_id: gid,
                });
            }
        }
    }

    manifests
}
