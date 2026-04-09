mod cli;
mod download;

use std::path::PathBuf;

use prost::Message;

use steam::client::DisconnectedClient;
use steam::client::msg::ClientMsg;
use steam::connection::DEFAULT_CM_SERVERS;
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
        Action::Files(ref args) => run_files(&opts, args).await,
        Action::Workshop(ref args) => run_workshop(&opts, args).await,
    }
}

// ── Shared connection helper ─────────────────────────────────

async fn connect_and_login(
    opts: &Options,
) -> Result<steam::client::SteamClient<steam::client::LoggedIn>, BoxError> {
    let (client, _events) = DisconnectedClient::new();
    let server = &DEFAULT_CM_SERVERS[0];
    tracing::info!("Connecting to {}...", server.addr);

    let client = client.connect(server).await?;
    tracing::info!("Connected, performing encryption handshake...");

    let client = client.encrypt().await?;
    tracing::info!("Encrypted, logging in...");

    let logon_body = build_logon_body(opts);
    let logon_msg = ClientMsg::with_body(EMsg::CLIENT_LOGON, &logon_body);

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

    let tokens = client.pics_get_access_tokens(&[app_id]).await?;
    let app_infos = client.pics_get_product_info(&tokens).await?;

    let cdn_servers = client.get_cdn_servers(cell_id, None).await?;
    if cdn_servers.is_empty() {
        return Err("No CDN servers available".into());
    }

    let depot_ids: Vec<DepotId> = if args.depot.is_empty() {
        discover_depots(&app_infos)
    } else {
        args.depot.iter().map(|&id| DepotId(id)).collect()
    };

    if depot_ids.is_empty() {
        return Err("No depots to download".into());
    }

    let manifest_ids: Vec<Option<ManifestId>> = if args.manifest.is_empty() {
        depot_ids.iter().map(|_| None).collect()
    } else {
        args.manifest.iter().map(|&id| Some(ManifestId(id))).collect()
    };

    let install_dir = args
        .output
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("depots"));

    let cdn = steam::cdn::CdnClient::new()?;

    for (i, &depot_id) in depot_ids.iter().enumerate() {
        tracing::info!("Processing depot {depot_id}...");

        let depot_key = client.get_depot_decryption_key(depot_id, app_id).await?;

        let manifest_id = manifest_ids
            .get(i)
            .copied()
            .flatten()
            .ok_or_else(|| format!("No manifest ID for depot {depot_id}"))?;

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

        let manifest = steam_client::manifest::extract_and_parse(&manifest_bytes)?;

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
            .build()
            .map_err(|e| -> BoxError { e.into() })?;

        job.download(&manifest).await?;
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

    let tokens = client.pics_get_access_tokens(&[app_id]).await?;
    let app_infos = client.pics_get_product_info(&tokens).await?;

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

// ── files ────────────────────────────────────────────────────

async fn run_files(opts: &Options, args: &cli::FilesArgs) -> Result<(), BoxError> {
    let app_id = AppId(args.app);
    let depot_id = DepotId(args.depot);

    let client = connect_and_login(opts).await?;
    let cell_id = CellId(opts.cell_id.unwrap_or(0));

    let manifest_id = match args.manifest {
        Some(id) => ManifestId(id),
        None => {
            return Err(
                "Manifest ID discovery from branch not yet implemented. Pass --manifest explicitly."
                    .into(),
            );
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

    let manifest = steam_client::manifest::extract_and_parse(&manifest_bytes)?;

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

/// Depot info extracted from PICS KV data.
#[derive(Debug, serde::Serialize)]
struct DepotInfo {
    id: DepotId,
    name: Option<String>,
}

/// Extract depot IDs from PICS app info KV data.
fn discover_depots(app_infos: &[steam::apps::AppInfo]) -> Vec<DepotId> {
    discover_depot_details(app_infos)
        .into_iter()
        .map(|d| d.id)
        .collect()
}

/// Extract depot IDs and names from PICS app info KV data.
fn discover_depot_details(app_infos: &[steam::apps::AppInfo]) -> Vec<DepotInfo> {
    use steam::types::key_value::{KvValue, parse_binary_kv};

    let mut depots = Vec::new();

    for info in app_infos {
        let kv_data = match &info.kv_data {
            Some(data) => data,
            None => continue,
        };

        let mut input = kv_data.as_slice();
        let kv = match parse_binary_kv(&mut input) {
            Ok(kv) => kv,
            Err(_) => continue,
        };

        let depots_section = match kv.get("depots") {
            Some(d) => d,
            None => continue,
        };

        if let KvValue::Children(children) = &depots_section.value {
            for (key, value) in children {
                if let Ok(id) = key.parse::<u32>() {
                    let name = value.get("name").and_then(|n| n.as_str()).map(String::from);
                    depots.push(DepotInfo {
                        id: DepotId(id),
                        name,
                    });
                }
            }
        }
    }

    depots
}
