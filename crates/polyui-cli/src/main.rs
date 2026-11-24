use clap::{Parser, Subcommand, ValueEnum};
use polyui_core::tools::{self, ToolOptions};
use polyui_core::{ActionTable, CatalogSet, Locale, StaticRefs, Validator};
use polyui_server::anthropic::{self, AnthropicProvider, ApiKey};
use polyui_server::bills::{self, BillsHost};
use polyui_server::{Agent, Host, ServerConfig, User};
use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

#[derive(Debug, Parser)]
#[command(
    name = "polyui",
    version,
    about = "Run the PolyUI reference server and check model output against the catalogs."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum HostChoice {
    Bills,
    #[cfg(feature = "query")]
    Shop,
    None,
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(about = "Serve the agent over WebSocket (/ws) and SSE (/events, /send).")]
    Serve {
        #[arg(long, default_value = "127.0.0.1:8787", help = "Address to listen on.")]
        addr: SocketAddr,
        #[arg(long = "static", value_name = "DIR", help = "Directory served at /.")]
        static_dir: Option<PathBuf>,
        #[arg(
            long,
            value_name = "PATH",
            help = "Dotenv style file that sets ANTHROPIC_API_KEY."
        )]
        env_file: Option<PathBuf>,
        #[arg(long, default_value = anthropic::DEFAULT_MODEL, help = "Anthropic model id.")]
        model: String,
        #[arg(long, value_enum, default_value = "bills", help = "Demo host to run.")]
        host: HostChoice,
        #[arg(
            long,
            value_name = "DIR",
            default_value = DEFAULT_DATA_DIR,
            help = "Data directory for the shop host."
        )]
        data: PathBuf,
    },
    #[command(about = "Print the show_ui, update_ui and close_ui tool definitions as JSON.")]
    Tools {
        #[arg(
            long,
            value_delimiter = ',',
            default_value = "core@1,chart@1",
            help = "Catalogs the client declares."
        )]
        catalogs: Vec<String>,
        #[arg(long, help = "Strict mode: no length or range limits in the schema.")]
        strict: bool,
        #[arg(
            long,
            value_enum,
            default_value = "bills",
            help = "Whose actions the schema lists."
        )]
        host: HostChoice,
        #[arg(
            long,
            value_name = "DIR",
            default_value = DEFAULT_DATA_DIR,
            help = "Data directory for the shop host."
        )]
        data: PathBuf,
    },
    #[command(about = "Check a show_ui input file and print its problems or its summary.")]
    Validate {
        #[arg(help = "JSON file with the show_ui input.")]
        file: PathBuf,
        #[arg(
            long,
            value_enum,
            default_value = "bills",
            help = "Whose actions and refs to check against."
        )]
        host: HostChoice,
        #[arg(
            long,
            value_name = "DIR",
            default_value = DEFAULT_DATA_DIR,
            help = "Data directory for the shop host."
        )]
        data: PathBuf,
    },
}

#[cfg(feature = "query")]
const DEFAULT_DATA_DIR: &str = polyui_server::shop::DEFAULT_DATA_DIR;
#[cfg(not(feature = "query"))]
const DEFAULT_DATA_DIR: &str = "examples/data/shop";

async fn host_for(choice: HostChoice, data: &Path) -> Result<Option<Arc<dyn Host>>, String> {
    let _ = data;
    match choice {
        HostChoice::Bills => Ok(Some(Arc::new(BillsHost::new()))),
        #[cfg(feature = "query")]
        HostChoice::Shop => polyui_server::shop::ShopHost::open(data)
            .await
            .map(|host| Some(Arc::new(host) as Arc<dyn Host>))
            .map_err(|error| format!("cannot open the shop data in {}: {error}", data.display())),
        HostChoice::None => Ok(None),
    }
}

fn data_tools(host: &Arc<dyn Host>) -> Vec<serde_json::Value> {
    #[cfg(feature = "query")]
    if host.data().is_some() {
        return polyui_server::dashboards::tools();
    }
    let _ = host;
    Vec::new()
}

async fn print_tools(
    catalogs: &[String],
    strict: bool,
    choice: HostChoice,
    data: &Path,
) -> ExitCode {
    let full = CatalogSet::builtin();
    let options = ToolOptions { strict };
    let empty = ActionTable::new();
    let host = match host_for(choice, data).await {
        Ok(host) => host,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    let mut list = match &host {
        Some(host) => tools::for_client(&full, catalogs, host.actions(), options),
        None => tools::for_client(&full, catalogs, &empty, options),
    };
    if let (Some(host), serde_json::Value::Array(items)) = (&host, &mut list) {
        items.extend(host.tools());
        items.extend(data_tools(host));
    }
    match serde_json::to_string_pretty(&list) {
        Ok(text) => match writeln!(std::io::stdout().lock(), "{text}") {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::FAILURE,
        },
        Err(error) => {
            eprintln!("could not print the tools: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn validate_file(file: &PathBuf, choice: HostChoice, data: &Path) -> ExitCode {
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("cannot read {}: {error}", file.display());
            return ExitCode::FAILURE;
        }
    };
    let catalogs = CatalogSet::builtin();
    let empty_actions = ActionTable::new();
    let empty_refs = StaticRefs::new();
    let host = match host_for(choice, data).await {
        Ok(host) => host,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    let user = User::new(bills::HOUSEHOLD_USER);
    let refs = host.as_ref().map(|host| host.refs(&user));
    let validator = match (&host, &refs) {
        (Some(host), Some(refs)) => Validator::new(&catalogs, host.actions(), refs.as_ref()),
        _ => Validator::new(&catalogs, &empty_actions, &empty_refs),
    };
    match validator.show_ui_str(&text) {
        Ok(surface) => {
            let id = surface.id.clone().unwrap_or_else(|| "s1".into());
            println!("valid");
            println!("{}", surface.summary(&id, &catalogs, Locale::En));
            ExitCode::SUCCESS
        }
        Err(problems) => {
            println!("{problems}");
            ExitCode::FAILURE
        }
    }
}

async fn serve(
    addr: SocketAddr,
    static_dir: Option<PathBuf>,
    env_file: Option<PathBuf>,
    model: String,
    choice: HostChoice,
    data: PathBuf,
) -> ExitCode {
    let key = match &env_file {
        Some(path) => ApiKey::from_env_file(path),
        None => ApiKey::from_env()
            .ok_or_else(|| format!("set {} or pass --env-file", anthropic::KEY_VARIABLE)),
    };
    let key = match key {
        Ok(key) => key,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    let host = match host_for(choice, &data).await {
        Ok(Some(host)) => host,
        Ok(None) => {
            eprintln!("serve needs a host; use --host bills or --host shop");
            return ExitCode::FAILURE;
        }
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    let provider = Arc::new(AnthropicProvider::new(key, model.clone()));
    let agent = Agent::new(host, provider);
    let app = polyui_server::router(
        agent,
        ServerConfig {
            static_dir: static_dir.clone(),
            ..ServerConfig::default()
        },
    );
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("cannot listen on {addr}: {error}");
            return ExitCode::FAILURE;
        }
    };
    println!("PolyUI is listening on http://{addr} (model {model})");
    println!("WebSocket: ws://{addr}/ws  SSE: http://{addr}/events  POST: http://{addr}/send");
    if let Some(dir) = &static_dir {
        println!("Static files from {}", dir.display());
    }
    match polyui_server::serve(listener, app).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("server stopped: {error}");
            ExitCode::FAILURE
        }
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Serve {
            addr,
            static_dir,
            env_file,
            model,
            host,
            data,
        } => serve(addr, static_dir, env_file, model, host, data).await,
        Command::Tools {
            catalogs,
            strict,
            host,
            data,
        } => print_tools(&catalogs, strict, host, &data).await,
        Command::Validate { file, host, data } => validate_file(&file, host, &data).await,
    }
}
