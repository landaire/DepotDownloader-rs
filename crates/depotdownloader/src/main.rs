mod cli;
mod download;

use std::path::PathBuf;

use prost::Message;

use steam::client::DisconnectedClient;
use steam::client::msg::ClientMsg;
use steam::connection::DEFAULT_CM_SERVERS;
use steam::depot::{AppId, CellId, DepotId, ManifestId};
use steam::messages::EMsg;

use crate::cli::{Action, Options};

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
        Action::Manifest(ref args) => run_manifest(&opts, args).await,
        Action::Workshop(ref args) => run_workshop(&opts, args).await,
    }
}

async fn connect_and_login(opts: &Options) -> Result<steam::client::SteamClient<steam::client::LoggedIn>, BoxError> {
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

async fn run_download(opts: &Options, args: &cli::DownloadArgs) -> Result<(), BoxError> {
    let app_id = AppId(args.app);
    tracing::info!("Downloading app {app_id}");

    let client = connect_and_login(opts).await?;
    let cell_id = CellId(opts.cell_id.unwrap_or(0));

    let tokens = client.pics_get_access_tokens(&[app_id]).await?;
    tracing::info!("Got {} access token(s)", tokens.len());

    let app_infos = client.pics_get_product_info(&tokens).await?;
    tracing::info!("Got product info for {} app(s)", app_infos.len());

    let cdn_servers = client.get_cdn_servers(cell_id, None).await?;
    tracing::info!("Got {} CDN server(s)", cdn_servers.len());

    if cdn_servers.is_empty() {
        return Err("No CDN servers available".into());
    }

    let depot_ids: Vec<DepotId> = if args.depot.is_empty() {
        tracing::info!("No specific depots requested, discovering from app info...");
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
        tracing::info!("Got depot key for {depot_id}");

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
            "Manifest has {} files, {} total bytes",
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

async fn run_manifest(opts: &Options, args: &cli::ManifestArgs) -> Result<(), BoxError> {
    let app_id = AppId(args.app);
    let depot_id = DepotId(args.depot);

    tracing::info!("Inspecting manifest for app {app_id} depot {depot_id}");

    let client = connect_and_login(opts).await?;
    let cell_id = CellId(opts.cell_id.unwrap_or(0));

    let cdn_servers = client.get_cdn_servers(cell_id, None).await?;
    if cdn_servers.is_empty() {
        return Err("No CDN servers available".into());
    }

    // If no manifest ID given, get it from PICS
    let manifest_id = match args.manifest {
        Some(id) => ManifestId(id),
        None => {
            return Err("Manifest ID discovery from branch not yet implemented. Pass --manifest explicitly.".into());
        }
    };

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

    println!("Depot:    {depot_id}");
    println!("Manifest: {manifest_id}");
    if let Some(t) = manifest.creation_time {
        println!("Created:  {t}");
    }
    println!("Files:    {}", manifest.files.len());
    if let Some(size) = manifest.total_uncompressed_size {
        println!("Size:     {size} bytes");
    }
    println!();

    for file in &manifest.files {
        let name = file.filename.as_deref().unwrap_or("<unnamed>");
        let size = file.size.map(|s| format!("{s}")).unwrap_or_else(|| "?".into());
        println!("{name}\t{size} bytes\t{} chunks", file.chunks.len());
    }

    Ok(())
}

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

fn discover_depots(app_infos: &[steam::apps::AppInfo]) -> Vec<DepotId> {
    use steam::types::key_value::parse_binary_kv;

    let mut depot_ids = Vec::new();

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

        let depots = match kv.get("depots") {
            Some(d) => d,
            None => continue,
        };

        if let steam::types::key_value::KvValue::Children(children) = &depots.value {
            for key in children.keys() {
                if let Ok(id) = key.parse::<u32>() {
                    depot_ids.push(DepotId(id));
                }
            }
        }
    }

    depot_ids
}
