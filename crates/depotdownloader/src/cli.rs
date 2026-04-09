use clap::{Parser, Subcommand, ValueEnum};

// ── Modern CLI (default) ─────────────────────────────────────

/// Steam depot content downloader.
#[derive(Debug, Parser)]
#[command(name = "depotdownloader", version, about)]
pub struct Cli {
    #[command(flatten)]
    pub auth: AuthOptions,

    #[command(subcommand)]
    pub command: Command,

    /// Enable debug logging.
    #[arg(long, global = true)]
    pub debug: bool,

    /// Steam cell ID (geographic region).
    #[arg(long, global = true)]
    pub cell_id: Option<u32>,

    /// Maximum concurrent downloads.
    #[arg(long, global = true, default_value = "8")]
    pub max_downloads: usize,

    /// Capture incoming network packets to a JSON file for replay testing.
    #[arg(long, global = true)]
    pub capture: Option<String>,
}

#[derive(Debug, Parser)]
pub struct AuthOptions {
    /// Steam username.
    #[arg(short, long)]
    pub username: Option<String>,

    /// Steam password (if omitted, will prompt).
    #[arg(short, long)]
    pub password: Option<String>,

    /// Use QR code for authentication.
    #[arg(long)]
    pub qr: bool,

    /// Remember login credentials for future sessions.
    #[arg(long)]
    pub remember_password: bool,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Download app depot content.
    Download(DownloadArgs),

    /// List depots available for an app.
    Depots(DepotsArgs),

    /// List available manifests (branches) for a depot.
    Manifests(ManifestsArgs),

    /// List files in a depot manifest.
    Files(FilesArgs),

    /// Download a Steam Workshop item.
    Workshop(WorkshopArgs),
}

/// Output format for list commands.
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum OutputFormat {
    /// Human-readable table output.
    #[default]
    Table,
    /// Machine-readable JSON output.
    Json,
}

#[derive(Debug, Parser)]
pub struct DownloadArgs {
    /// App ID to download.
    #[arg(short, long)]
    pub app: u32,

    /// Depot ID(s) to download. Omit to download all depots.
    #[arg(short, long, value_delimiter = ',')]
    pub depot: Vec<u32>,

    /// Manifest ID(s) for specific versions.
    #[arg(short, long, value_delimiter = ',')]
    pub manifest: Vec<u64>,

    /// Branch name.
    #[arg(short, long, default_value = "public")]
    pub branch: String,

    /// Beta password (if the branch requires one).
    #[arg(long)]
    pub beta_password: Option<String>,

    /// Output directory.
    #[arg(short, long)]
    pub output: Option<String>,

    /// Target OS filter (windows, macos, linux).
    #[arg(long)]
    pub os: Option<String>,

    /// Target architecture filter (32, 64).
    #[arg(long)]
    pub arch: Option<String>,

    /// Language filter.
    #[arg(long)]
    pub language: Option<String>,

    /// Download all platforms.
    #[arg(long)]
    pub all_platforms: bool,

    /// Download all architectures.
    #[arg(long)]
    pub all_archs: bool,

    /// Download all languages.
    #[arg(long)]
    pub all_languages: bool,

    /// Path to a file list (one filename per line) to filter downloads.
    #[arg(long)]
    pub filelist: Option<String>,

    /// Regex pattern to filter files.
    #[arg(long)]
    pub file_regex: Option<String>,

    /// Verify existing files against the manifest instead of downloading.
    #[arg(long)]
    pub verify: bool,

    /// Use Lancache for downloads.
    #[arg(long)]
    pub lancache: bool,

    /// A unique login ID for running multiple instances.
    #[arg(long)]
    pub login_id: Option<u32>,
}

/// List depots available for an app.
#[derive(Debug, Parser)]
pub struct DepotsArgs {
    /// App ID.
    #[arg(short, long)]
    pub app: u32,

    /// Output format.
    #[arg(short, long, value_enum, default_value_t)]
    pub format: OutputFormat,
}

/// List available manifests (branches) for a depot.
#[derive(Debug, Parser)]
pub struct ManifestsArgs {
    /// App ID.
    #[arg(short, long)]
    pub app: u32,

    /// Depot ID.
    #[arg(short, long)]
    pub depot: u32,

    /// Output format.
    #[arg(short, long, value_enum, default_value_t)]
    pub format: OutputFormat,
}

/// List files in a depot manifest.
#[derive(Debug, Parser)]
pub struct FilesArgs {
    /// App ID.
    #[arg(short, long)]
    pub app: u32,

    /// Depot ID.
    #[arg(short, long)]
    pub depot: u32,

    /// Manifest ID (if omitted, uses the latest for the branch).
    #[arg(short, long)]
    pub manifest: Option<u64>,

    /// Branch name.
    #[arg(short, long, default_value = "public")]
    pub branch: String,

    /// Output format.
    #[arg(short, long, value_enum, default_value_t)]
    pub format: OutputFormat,
}

#[derive(Debug, Parser)]
pub struct WorkshopArgs {
    /// Published file ID.
    #[arg(long)]
    pub pubfile: Option<u64>,

    /// UGC ID.
    #[arg(long)]
    pub ugc: Option<u64>,

    /// Output directory.
    #[arg(short, long)]
    pub output: Option<String>,
}

// ── Legacy compat CLI (DD_COMPAT=1) ──────────────────────────

/// DepotDownloader-compatible flat argument style.
///
/// Activated by setting the `DD_COMPAT=1` environment variable.
#[derive(Debug, Parser)]
#[command(name = "depotdownloader", version, about = "Steam depot content downloader (compat mode)")]
pub struct CompatCli {
    #[arg(short, long, alias = "user")]
    pub username: Option<String>,

    #[arg(short, long, alias = "pass")]
    pub password: Option<String>,

    #[arg(long)]
    pub qr: bool,

    #[arg(long)]
    pub remember_password: bool,

    #[arg(short, long)]
    pub app: Option<u32>,

    #[arg(short, long, value_delimiter = ',')]
    pub depot: Vec<u32>,

    #[arg(short, long, value_delimiter = ',')]
    pub manifest: Vec<u64>,

    #[arg(short, long, default_value = "public", alias = "beta")]
    pub branch: String,

    #[arg(long, alias = "betapassword")]
    pub beta_password: Option<String>,

    #[arg(long)]
    pub dir: Option<String>,

    #[arg(long)]
    pub os: Option<String>,

    #[arg(long)]
    pub arch: Option<String>,

    #[arg(long)]
    pub language: Option<String>,

    #[arg(long)]
    pub all_platforms: bool,

    #[arg(long)]
    pub all_archs: bool,

    #[arg(long)]
    pub all_languages: bool,

    #[arg(long, default_value = "8")]
    pub max_downloads: usize,

    #[arg(long)]
    pub manifest_only: bool,

    #[arg(long)]
    pub cell_id: Option<u32>,

    #[arg(long)]
    pub filelist: Option<String>,

    #[arg(long)]
    pub file_regex: Option<String>,

    #[arg(long, alias = "validate", alias = "verify_all")]
    pub verify: bool,

    #[arg(long)]
    pub pubfile: Option<u64>,

    #[arg(long)]
    pub ugc: Option<u64>,

    #[arg(long)]
    pub use_lancache: bool,

    #[arg(long)]
    pub login_id: Option<u32>,

    #[arg(long)]
    pub debug: bool,
}

// ── Resolved options (common to both modes) ──────────────────

/// Normalized options produced by either CLI mode.
///
/// The rest of the binary works against this — no branching on CLI mode.
#[derive(Debug)]
pub struct Options {
    pub auth: AuthOptions,
    pub action: Action,
    pub debug: bool,
    pub cell_id: Option<u32>,
    pub max_downloads: usize,
    pub capture: Option<String>,
}

#[derive(Debug)]
pub enum Action {
    Download(DownloadArgs),
    Depots(DepotsArgs),
    Manifests(ManifestsArgs),
    Files(FilesArgs),
    Workshop(WorkshopArgs),
}

impl Options {
    /// Parse from the environment, choosing CLI mode based on `DD_COMPAT`.
    pub fn parse() -> Self {
        if std::env::var("DD_COMPAT").is_ok_and(|v| v == "1") {
            Self::from_compat(CompatCli::parse())
        } else {
            Self::from_modern(Cli::parse())
        }
    }

    fn from_modern(cli: Cli) -> Self {
        let action = match cli.command {
            Command::Download(args) => Action::Download(args),
            Command::Depots(args) => Action::Depots(args),
            Command::Manifests(args) => Action::Manifests(args),
            Command::Files(args) => Action::Files(args),
            Command::Workshop(args) => Action::Workshop(args),
        };
        Self {
            auth: cli.auth,
            action,
            debug: cli.debug,
            cell_id: cli.cell_id,
            max_downloads: cli.max_downloads,
            capture: cli.capture,
        }
    }

    fn from_compat(cli: CompatCli) -> Self {
        let auth = AuthOptions {
            username: cli.username,
            password: cli.password,
            qr: cli.qr,
            remember_password: cli.remember_password,
        };

        let action = if cli.manifest_only {
            Action::Files(FilesArgs {
                app: cli.app.unwrap_or(0),
                depot: *cli.depot.first().unwrap_or(&0),
                manifest: cli.manifest.first().copied(),
                branch: cli.branch,
                format: OutputFormat::Table,
            })
        } else if cli.pubfile.is_some() || cli.ugc.is_some() {
            Action::Workshop(WorkshopArgs {
                pubfile: cli.pubfile,
                ugc: cli.ugc,
                output: cli.dir,
            })
        } else {
            Action::Download(DownloadArgs {
                app: cli.app.unwrap_or(0),
                depot: cli.depot,
                manifest: cli.manifest,
                branch: cli.branch,
                beta_password: cli.beta_password,
                output: cli.dir,
                os: cli.os,
                arch: cli.arch,
                language: cli.language,
                all_platforms: cli.all_platforms,
                all_archs: cli.all_archs,
                all_languages: cli.all_languages,
                filelist: cli.filelist,
                file_regex: cli.file_regex,
                verify: cli.verify,
                lancache: cli.use_lancache,
                login_id: cli.login_id,
            })
        };

        Self {
            auth,
            action,
            debug: cli.debug,
            cell_id: cli.cell_id,
            max_downloads: cli.max_downloads,
            capture: None,
        }
    }
}
