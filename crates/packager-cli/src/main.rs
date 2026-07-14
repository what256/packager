use clap::{Parser, Subcommand, ValueEnum};
use packager_core::{BuilderRequest, Engine};
use serde::Serialize;
use std::{path::PathBuf, process::Command};

#[derive(Parser)]
#[command(
    name = "packager",
    version,
    about = "Turn self-hosted software into ordinary local apps"
)]
struct Cli {
    /// Print machine-readable JSON.
    #[arg(long, global = true)]
    json: bool,
    /// Override Packager's shared application-data directory.
    #[arg(long, global = true, env = "PACKAGER_DATA_DIR")]
    data_dir: Option<PathBuf>,
    /// Override Packager's shared cache directory.
    #[arg(long, global = true, env = "PACKAGER_CACHE_DIR")]
    cache_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, ValueEnum)]
enum SourceKind {
    Compose,
    Image,
    Github,
}

impl SourceKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Compose => "compose",
            Self::Image => "image",
            Self::Github => "github",
        }
    }
}

#[derive(Subcommand)]
enum Commands {
    /// Show Packager and managed-runtime status.
    Status,
    /// List packages available in the built-in catalog.
    Catalog,
    /// List installed packaged apps.
    Apps,
    /// Install, start, stop, or inspect Packager's private runtime.
    Runtime {
        #[command(subcommand)]
        command: RuntimeCommand,
    },
    /// Install an app from the built-in catalog.
    Install { id: String },
    /// Import a folder containing packager.yml and compose.yml.
    Import { path: PathBuf },
    /// Inspect a Compose folder, container image, or public GitHub repository.
    Analyze {
        #[arg(value_enum)]
        kind: SourceKind,
        source: String,
    },
    /// Generate, validate, and install a shareable package.
    #[command(alias = "package")]
    Build {
        #[arg(value_enum)]
        kind: SourceKind,
        source: String,
        #[arg(long)]
        id: String,
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "")]
        description: String,
        #[arg(long, default_value = "https://github.com")]
        homepage: String,
        #[arg(long)]
        port: u16,
        /// Environment variable to generate and store securely; repeatable.
        #[arg(long = "secret")]
        secrets: Vec<String>,
    },
    /// Start an installed app.
    Start {
        id: String,
        /// Open the app in the default browser after starting.
        #[arg(long)]
        open: bool,
    },
    /// Open a ready app in the default browser.
    Open { id: String },
    /// Stop an installed app without deleting its data.
    Stop { id: String },
    /// Pull the latest images and recreate an app if it was running.
    Update { id: String },
    /// Print recent app logs.
    Logs {
        id: String,
        #[arg(short = 'n', long, default_value_t = 200)]
        lines: u32,
    },
    /// Remove an app, optionally deleting persistent data and secrets.
    Uninstall {
        id: String,
        #[arg(long)]
        delete_data: bool,
    },
    /// Configure or run image updates.
    AutoUpdates {
        #[command(subcommand)]
        command: AutoUpdateCommand,
    },
}

#[derive(Subcommand)]
enum RuntimeCommand {
    Status,
    Install,
    Start,
    Stop,
    /// Delete Packager's private VM, runtime tools, and container storage.
    #[command(alias = "remove")]
    Uninstall,
}

#[derive(Subcommand)]
enum AutoUpdateCommand {
    Enable { id: String },
    Disable { id: String },
    Run,
}

fn engine(cli: &Cli) -> Result<Engine, String> {
    match &cli.data_dir {
        Some(data) => Engine::new(
            data,
            cli.cache_dir.clone().unwrap_or_else(|| data.join("cache")),
        ),
        None => Engine::from_environment(),
    }
}

fn print<T: Serialize>(value: &T, json: bool) -> Result<(), String> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(value)
                .map_err(|error| format!("Cannot serialize output: {error}"))?
        );
    } else {
        let value = serde_json::to_value(value)
            .map_err(|error| format!("Cannot serialize output: {error}"))?;
        match value {
            serde_json::Value::String(text) => println!("{text}"),
            serde_json::Value::Array(items) => {
                if items.is_empty() {
                    println!("No results.");
                }
                for item in items {
                    if let Some(name) = item.get("name").and_then(|value| value.as_str()) {
                        let id = item
                            .get("id")
                            .and_then(|value| value.as_str())
                            .unwrap_or("");
                        let status = item
                            .get("status")
                            .and_then(|value| value.as_str())
                            .unwrap_or("");
                        println!("{name}\t{id}\t{status}");
                    } else if let Some(message) =
                        item.get("message").and_then(|value| value.as_str())
                    {
                        println!("{message}");
                    } else {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&item).unwrap_or_default()
                        );
                    }
                }
            }
            serde_json::Value::Object(ref object) => {
                if let Some(message) = object.get("message").and_then(|value| value.as_str()) {
                    println!("{message}");
                } else {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&value).unwrap_or_default()
                    );
                }
            }
            _ => println!(
                "{}",
                serde_json::to_string_pretty(&value).unwrap_or_default()
            ),
        }
    }
    Ok(())
}

fn open_browser(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let status = Command::new("open").arg(url).status();
    #[cfg(target_os = "windows")]
    let status = Command::new("cmd").args(["/C", "start", "", url]).status();
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let status = Command::new("xdg-open").arg(url).status();
    let status = status.map_err(|error| format!("Cannot open the default browser: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("The default browser could not open {url}"))
    }
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();
    let engine = engine(&cli)?;
    match cli.command {
        Commands::Status => print(&packager_core::system_status(&engine)?, cli.json),
        Commands::Catalog => print(&packager_core::catalog(&engine)?, cli.json),
        Commands::Apps => print(&packager_core::list_apps(&engine)?, cli.json),
        Commands::Runtime { command } => match command {
            RuntimeCommand::Status => print(&packager_core::runtime_status(&engine)?, cli.json),
            RuntimeCommand::Install => print(&packager_core::install_runtime(&engine)?, cli.json),
            RuntimeCommand::Start => print(&packager_core::start_runtime(&engine)?, cli.json),
            RuntimeCommand::Stop => print(&packager_core::stop_runtime(&engine)?, cli.json),
            RuntimeCommand::Uninstall => {
                print(&packager_core::uninstall_runtime(&engine)?, cli.json)
            }
        },
        Commands::Install { id } => print(&packager_core::install(&engine, &id)?, cli.json),
        Commands::Import { path } => print(
            &packager_core::import_package(&engine, &path.to_string_lossy())?,
            cli.json,
        ),
        Commands::Analyze { kind, source } => print(
            &packager_core::analyze(&engine, kind.as_str(), &source)?,
            cli.json,
        ),
        Commands::Build {
            kind,
            source,
            id,
            name,
            description,
            homepage,
            port,
            secrets,
        } => print(
            &packager_core::build(
                &engine,
                BuilderRequest {
                    source_kind: kind.as_str().into(),
                    source,
                    id,
                    name,
                    description,
                    homepage,
                    container_port: port,
                    secret_keys: secrets,
                },
            )?,
            cli.json,
        ),
        Commands::Start { id, open } => {
            let result = packager_core::start(&engine, &id)?;
            print(&result, cli.json)?;
            if open {
                open_browser(&packager_core::app_url(&engine, &id)?)?;
            }
            Ok(())
        }
        Commands::Open { id } => open_browser(&packager_core::app_url(&engine, &id)?),
        Commands::Stop { id } => print(&packager_core::stop(&engine, &id)?, cli.json),
        Commands::Update { id } => print(&packager_core::update(&engine, &id)?, cli.json),
        Commands::Logs { id, lines } => print(&packager_core::logs(&engine, &id, lines)?, cli.json),
        Commands::Uninstall { id, delete_data } => print(
            &packager_core::uninstall(&engine, &id, delete_data)?,
            cli.json,
        ),
        Commands::AutoUpdates { command } => match command {
            AutoUpdateCommand::Enable { id } => print(
                &packager_core::set_automatic_updates(&engine, &id, true)?,
                cli.json,
            ),
            AutoUpdateCommand::Disable { id } => print(
                &packager_core::set_automatic_updates(&engine, &id, false)?,
                cli.json,
            ),
            AutoUpdateCommand::Run => print(&packager_core::automatic_updates(&engine)?, cli.json),
        },
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("packager: {error}");
        std::process::exit(1);
    }
}
