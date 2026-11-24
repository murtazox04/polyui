use clap::{Parser, Subcommand, ValueEnum};
use polyui_core::tools::{self, ToolOptions};
use polyui_core::{ActionTable, CatalogSet, Locale, StaticRefs, Validator};
use polyui_server::anthropic::{self, AnthropicProvider, ApiKey};
use polyui_server::bills::{self, BillsHost};
use polyui_server::openai::{self, OpenAiCompatProvider};
use polyui_server::{Agent, Host, ModelProvider, ServerConfig, User};
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ProviderChoice {
    Anthropic,
    OpenaiCompat,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum HostChoice {
    Bills,
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
            help = "Dotenv style file that sets the API key (ANTHROPIC_API_KEY, or the --api-key-env variable for openai-compat)."
        )]
        env_file: Option<PathBuf>,
        #[arg(
            long,
            help = "Model id. Defaults to claude-haiku-5-5 for anthropic; required for openai-compat."
        )]
        model: Option<String>,
        #[arg(
            long,
            value_enum,
            default_value = "anthropic",
            help = "Model API: the Anthropic Messages API, or any OpenAI-compatible /chat/completions endpoint."
        )]
        provider: ProviderChoice,
        #[arg(
            long,
            value_name = "URL",
            help = "Base URL of the OpenAI-compatible API, for example http://127.0.0.1:8080/v1."
        )]
        base_url: Option<String>,
        #[arg(
            long,
            value_name = "NAME",
            default_value = openai::DEFAULT_KEY_VARIABLE,
            help = "Environment variable (or --env-file entry) with the bearer key for openai-compat. Without it, requests go without a key."
        )]
        api_key_env: String,
        #[arg(long, value_enum, default_value = "bills", help = "Demo host to run.")]
        host: HostChoice,
        #[arg(
            long,
            value_name = "DIR",
            default_value = DEFAULT_DATA_DIR,
            help = "Data directory for the shop host."
        )]
        data: PathBuf,
        #[arg(
            long,
            value_name = "URL",
            help = "Origin of the MCP App sandbox page. Adds the tariff calculator app to the bills host and tells the chat page where the sandbox is. An http URL on localhost, 127.0.0.1 or [::1] is served from --embed-sandbox on that port."
        )]
        embed_origin: Option<String>,
        #[arg(
            long,
            value_name = "DIR",
            default_value = DEFAULT_SANDBOX_DIR,
            help = "Directory with the sandbox page, served at a local --embed-origin."
        )]
        embed_sandbox: PathBuf,
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

const DEFAULT_SANDBOX_DIR: &str = "js/polyui-web/embed-sandbox";

#[cfg(feature = "query")]
const DEFAULT_DATA_DIR: &str = polyui_server::shop::DEFAULT_DATA_DIR;
#[cfg(not(feature = "query"))]
const DEFAULT_DATA_DIR: &str = "examples/data/shop";

#[cfg(not(feature = "query"))]
const SHOP_NEEDS_QUERY: &str = "the shop host needs the query feature, which this polyui binary was built without; rebuild it with `cargo install --path crates/polyui-cli --features query` or run `cargo run -p polyui --features query -- serve --host shop`";

#[cfg(feature = "query")]
async fn shop_host(data: &Path) -> Result<Option<Arc<dyn Host>>, String> {
    polyui_server::shop::ShopHost::open(data)
        .await
        .map(|host| Some(Arc::new(host) as Arc<dyn Host>))
        .map_err(|error| format!("cannot open the shop data in {}: {error}", data.display()))
}

#[cfg(not(feature = "query"))]
async fn shop_host(_: &Path) -> Result<Option<Arc<dyn Host>>, String> {
    Err(SHOP_NEEDS_QUERY.to_string())
}

async fn host_for(
    choice: HostChoice,
    data: &Path,
    tariff_app: bool,
) -> Result<Option<Arc<dyn Host>>, String> {
    match choice {
        HostChoice::Bills if tariff_app => Ok(Some(Arc::new(BillsHost::new().with_tariff_app()))),
        HostChoice::Bills => Ok(Some(Arc::new(BillsHost::new()))),
        HostChoice::Shop => shop_host(data).await,
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
    let host = match host_for(choice, data, false).await {
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
    let host = match host_for(choice, data, false).await {
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

fn sandbox_address(origin: &str, app: SocketAddr) -> Result<Option<SocketAddr>, String> {
    if origin.starts_with("https://") {
        return Ok(None);
    }
    let Some(rest) = origin.strip_prefix("http://") else {
        return Err(format!(
            "--embed-origin must be an https:// or http:// URL, not {origin}"
        ));
    };
    let authority = rest.split('/').next().unwrap_or_default();
    let (host, port) = authority.rsplit_once(':').ok_or_else(|| {
        format!("--embed-origin {origin} needs a port, for example http://localhost:8788/")
    })?;
    let ip = match host {
        "localhost" | "127.0.0.1" => IpAddr::V4(Ipv4Addr::LOCALHOST),
        "[::1]" => IpAddr::V6(Ipv6Addr::LOCALHOST),
        _ => {
            return Err(format!(
                "an http --embed-origin must be on localhost, 127.0.0.1 or [::1], not {host}; use https for anything else"
            ));
        }
    };
    let port = port
        .parse::<u16>()
        .map_err(|_| format!("--embed-origin {origin} has no valid port"))?;
    let sandbox = SocketAddr::new(ip, port);
    if port == app.port() && (app.ip().is_unspecified() || app.ip().is_loopback()) {
        return Err(format!(
            "the sandbox needs an origin of its own; --embed-origin {origin} uses the port the app listens on ({app})"
        ));
    }
    Ok(Some(sandbox))
}

async fn sandbox_listener(
    origin: &str,
    app: SocketAddr,
    dir: &Path,
) -> Result<Option<tokio::net::TcpListener>, String> {
    let Some(sandbox) = sandbox_address(origin, app)? else {
        return Ok(None);
    };
    if !dir.join("index.html").is_file() {
        return Err(format!(
            "{} has no index.html; pass the sandbox directory with --embed-sandbox",
            dir.display()
        ));
    }
    tokio::net::TcpListener::bind(sandbox)
        .await
        .map(Some)
        .map_err(|error| format!("cannot listen on {sandbox} for the sandbox: {error}"))
}

struct ModelOptions {
    provider: ProviderChoice,
    env_file: Option<PathBuf>,
    model: Option<String>,
    base_url: Option<String>,
    api_key_env: String,
}

fn anthropic_key(env_file: Option<&Path>) -> Result<ApiKey, String> {
    match env_file {
        Some(path) => ApiKey::from_env_file(path),
        None => ApiKey::from_env()
            .ok_or_else(|| format!("set {} or pass --env-file", anthropic::KEY_VARIABLE)),
    }
}

fn compat_key(env_file: Option<&Path>, variable: &str) -> Result<Option<ApiKey>, String> {
    match env_file {
        Some(path) => ApiKey::from_env_file_var(path, variable).map(Some),
        None => Ok(ApiKey::from_env_var(variable)),
    }
}

fn model_provider(options: ModelOptions) -> Result<(Arc<dyn ModelProvider>, String), String> {
    let ModelOptions {
        provider,
        env_file,
        model,
        base_url,
        api_key_env,
    } = options;
    match provider {
        ProviderChoice::Anthropic => {
            if base_url.is_some() {
                return Err("--base-url is only used with --provider openai-compat".into());
            }
            let model = model.unwrap_or_else(|| anthropic::DEFAULT_MODEL.to_string());
            let key = anthropic_key(env_file.as_deref())?;
            Ok((Arc::new(AnthropicProvider::new(key, model.clone())), model))
        }
        ProviderChoice::OpenaiCompat => {
            let base_url = base_url.ok_or(
                "--provider openai-compat needs --base-url, for example http://127.0.0.1:8080/v1",
            )?;
            let model = model.ok_or("--provider openai-compat needs --model")?;
            let mut compat = OpenAiCompatProvider::new(&base_url, model.clone());
            match compat_key(env_file.as_deref(), &api_key_env)? {
                Some(key) => compat = compat.with_key(key),
                None => eprintln!("{api_key_env} is not set; sending requests without a key"),
            }
            let label = format!("{model} at {}", compat.endpoint());
            Ok((Arc::new(compat), label))
        }
    }
}

struct ServeOptions {
    addr: SocketAddr,
    static_dir: Option<PathBuf>,
    model: ModelOptions,
    choice: HostChoice,
    data: PathBuf,
    embed_origin: Option<String>,
    embed_sandbox: PathBuf,
}

async fn serve(options: ServeOptions) -> ExitCode {
    let ServeOptions {
        addr,
        static_dir,
        model,
        choice,
        data,
        embed_origin,
        embed_sandbox,
    } = options;
    let tariff_app = embed_origin.is_some();
    if tariff_app && !matches!(choice, HostChoice::Bills) {
        eprintln!("--embed-origin only adds an MCP App to the bills host; this host has none");
    }
    let sandbox = match &embed_origin {
        Some(origin) => match sandbox_listener(origin, addr, &embed_sandbox).await {
            Ok(listener) => listener,
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::FAILURE;
            }
        },
        None => None,
    };
    let host = match host_for(choice, &data, tariff_app).await {
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
    let (provider, model) = match model_provider(model) {
        Ok(chosen) => chosen,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    let agent = Agent::new(host, provider);
    let app = polyui_server::router(
        agent,
        ServerConfig {
            static_dir: static_dir.clone(),
            embed_origin: embed_origin.clone(),
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
    if let Some(origin) = &embed_origin {
        match &sandbox {
            Some(_) => println!(
                "MCP App sandbox: {origin} (files from {})",
                embed_sandbox.display()
            ),
            None => println!("MCP App sandbox: {origin} (served elsewhere)"),
        }
    }
    let served = match sandbox {
        Some(sandbox) => {
            let pages = polyui_server::static_router(embed_sandbox);
            tokio::select! {
                result = polyui_server::serve(listener, app) => result,
                result = polyui_server::serve(sandbox, pages) => result,
            }
        }
        None => polyui_server::serve(listener, app).await,
    };
    match served {
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
            provider,
            base_url,
            api_key_env,
            host,
            data,
            embed_origin,
            embed_sandbox,
        } => {
            serve(ServeOptions {
                addr,
                static_dir,
                model: ModelOptions {
                    provider,
                    env_file,
                    model,
                    base_url,
                    api_key_env,
                },
                choice: host,
                data,
                embed_origin,
                embed_sandbox,
            })
            .await
        }
        Command::Tools {
            catalogs,
            strict,
            host,
            data,
        } => print_tools(&catalogs, strict, host, &data).await,
        Command::Validate { file, host, data } => validate_file(&file, host, &data).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], 8787))
    }

    fn compat(base_url: Option<&str>, model: Option<&str>) -> ModelOptions {
        ModelOptions {
            provider: ProviderChoice::OpenaiCompat,
            env_file: None,
            model: model.map(str::to_string),
            base_url: base_url.map(str::to_string),
            api_key_env: "POLYUI_TEST_KEY_THAT_IS_NOT_SET".into(),
        }
    }

    #[test]
    fn openai_compat_needs_a_base_url_and_a_model() {
        let missing_url = model_provider(compat(None, Some("m"))).err();
        assert!(missing_url.is_some_and(|message| message.contains("--base-url")));
        let missing_model = model_provider(compat(Some("http://127.0.0.1:8080/v1"), None)).err();
        assert!(missing_model.is_some_and(|message| message.contains("--model")));
        let chosen = model_provider(compat(Some("http://127.0.0.1:8080/v1/"), Some("gemma")));
        assert_eq!(
            chosen.map(|(_, label)| label).ok(),
            Some("gemma at http://127.0.0.1:8080/v1/chat/completions".to_string())
        );
        let cli = Cli::try_parse_from([
            "polyui",
            "serve",
            "--provider",
            "openai-compat",
            "--base-url",
            "http://127.0.0.1:8080/v1",
            "--model",
            "gemma",
            "--api-key-env",
            "MY_KEY",
        ]);
        assert!(matches!(
            cli.map(|cli| cli.command),
            Ok(Command::Serve { provider: ProviderChoice::OpenaiCompat, ref api_key_env, .. }) if api_key_env == "MY_KEY"
        ));
    }

    #[test]
    fn local_http_sandboxes_get_their_own_port() {
        assert_eq!(
            sandbox_address("http://localhost:8788/", app()),
            Ok(Some(SocketAddr::from(([127, 0, 0, 1], 8788))))
        );
        assert_eq!(
            sandbox_address("http://[::1]:9000", app()),
            Ok(Some(SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 9000)))
        );
        assert_eq!(
            sandbox_address("https://sandbox.example-usercontent.net/", app()),
            Ok(None)
        );
        assert!(sandbox_address("http://127.0.0.1:8787/", app()).is_err());
        assert!(sandbox_address("http://example.com:8788/", app()).is_err());
        assert!(sandbox_address("http://localhost/", app()).is_err());
        assert!(sandbox_address("ftp://localhost:21/", app()).is_err());
    }
}
