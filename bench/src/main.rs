mod meter;
mod prompts;
mod report;
mod run;

use clap::{Parser, ValueEnum};
use futures_util::StreamExt;
use futures_util::future::BoxFuture;
use meter::{Budget, Pacer};
use polyui_core::tools::{self, ToolProfile};
use polyui_decode::SchemaDialect;
use polyui_server::anthropic::{self, AnthropicProvider, ApiKey};
use polyui_server::bills::{BillsHost, HOUSEHOLD_USER};
use polyui_server::openai::{self, OpenAiCompatProvider, json_schema_format};
use polyui_server::{Agent, AgentConfig, Host, ModelProvider, User};
use prompts::Prompt;
use report::{Calls, Prices, Report, ToolSetup};
use run::{Bench, RunRecord, Variant};
use serde_json::Value;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ProviderChoice {
    Anthropic,
    OpenaiCompat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ProfileChoice {
    Compact,
    Full,
}

impl From<ProfileChoice> for ToolProfile {
    fn from(choice: ProfileChoice) -> Self {
        match choice {
            ProfileChoice::Compact => Self::Compact,
            ProfileChoice::Full => Self::Full,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum DialectChoice {
    Standard,
    NoSameTypeUnions,
}

impl From<DialectChoice> for SchemaDialect {
    fn from(choice: DialectChoice) -> Self {
        match choice {
            DialectChoice::Standard => Self::Standard,
            DialectChoice::NoSameTypeUnions => Self::NoSameTypeUnions,
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "polyui-bench",
    about = "Measures how often the model produces a valid show_ui on the first try and after retries."
)]
struct Args {
    #[arg(long, value_enum, default_value = "anthropic")]
    provider: ProviderChoice,
    #[arg(
        long,
        help = "Model id; defaults to claude-haiku-5-5 for anthropic and is required for openai-compat"
    )]
    model: Option<String>,
    #[arg(
        long,
        help = "Base URL of the OpenAI-compatible API, for openai-compat"
    )]
    base_url: Option<String>,
    #[arg(
        long,
        default_value = openai::DEFAULT_KEY_VARIABLE,
        help = "Variable with the bearer key for openai-compat, read from the environment or --env-file"
    )]
    api_key_env: String,
    #[arg(
        long,
        help = "Dotenv file with the API key; without it the key comes from the environment"
    )]
    env_file: Option<PathBuf>,
    #[arg(
        long,
        value_enum,
        value_delimiter = ',',
        default_value = "tools,inline",
        help = "Settings to run, comma separated: tools, constrained, inline"
    )]
    variant: Vec<Variant>,
    #[arg(
        long,
        value_enum,
        default_value = "compact",
        help = "Tool profile for the prompt: compact (one line per type) or full (every prop in the schema); constrained grammars always use full"
    )]
    tool_profile: ProfileChoice,
    #[arg(
        long,
        value_enum,
        default_value = "no-same-type-unions",
        help = "JSON Schema dialect of the constrained reply schema: no-same-type-unions merges anyOf branches of the same JSON type, for servers such as ember that refuse them; standard keeps every union"
    )]
    schema_dialect: DialectChoice,
    #[arg(
        long,
        value_delimiter = ',',
        help = "Offer the model only these component types, comma separated; all types when not given"
    )]
    catalog_subset: Vec<String>,
    #[arg(
        long,
        value_delimiter = ',',
        help = "Run only these prompt ids, comma separated, for example en-phone-form,ru-phone-form"
    )]
    only: Vec<String>,
    #[arg(long, help = "Run only the first N prompts")]
    limit: Option<usize>,
    #[arg(long, default_value_t = 4)]
    concurrency: usize,
    #[arg(
        long,
        default_value_t = 160,
        help = "Hard cap on model calls for the whole run"
    )]
    max_calls: usize,
    #[arg(
        long,
        default_value_t = 16000,
        help = "max_tokens for every model call"
    )]
    max_tokens: u32,
    #[arg(
        long,
        default_value_t = 0,
        help = "Least time between the starts of two model calls, in milliseconds"
    )]
    min_gap_ms: u64,
    #[arg(
        long,
        default_value_t = 3,
        help = "Stop calling the model after this many provider errors in a row; 0 never stops"
    )]
    stop_after_errors: usize,
    #[arg(
        long,
        default_value_t = 2,
        help = "Tries per call when an openai-compat server answers 502, 503 or 504; 1 never retries"
    )]
    server_error_attempts: u32,
    #[arg(
        long,
        help = "Sampling temperature for openai-compat; the server default without it"
    )]
    temperature: Option<f64>,
    #[arg(long, default_value = "bench/results")]
    out_dir: PathBuf,
    #[arg(
        long,
        help = "File name stem for the results; defaults to <date>-<model>"
    )]
    name: Option<String>,
    #[arg(long, default_value_t = 0.10)]
    input_price: f64,
    #[arg(long, default_value_t = 0.50)]
    output_price: f64,
    #[arg(long, default_value_t = 1.25)]
    cache_write_factor: f64,
    #[arg(long, default_value_t = 0.1)]
    cache_read_factor: f64,
    #[arg(long, help = "Print the prompt set and exit without calling the model")]
    list: bool,
}

fn civil_date(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (
        year,
        u32::try_from(month).unwrap_or(1),
        u32::try_from(day).unwrap_or(1),
    )
}

fn today() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default();
    let days = i64::try_from(seconds / 86_400).unwrap_or_default();
    let (year, month, day) = civil_date(days);
    format!("{year:04}-{month:02}-{day:02}")
}

struct Providers {
    plain: Arc<dyn ModelProvider>,
    constrained: Option<Arc<dyn ModelProvider>>,
    compat: Option<OpenAiCompatProvider>,
    model: String,
}

fn anthropic_key(args: &Args) -> Result<ApiKey, String> {
    match &args.env_file {
        Some(path) => ApiKey::from_env_file(path),
        None => ApiKey::from_env().ok_or_else(|| format!("{} is not set", anthropic::KEY_VARIABLE)),
    }
}

fn compat_key(args: &Args) -> Result<Option<ApiKey>, String> {
    match &args.env_file {
        Some(path) => ApiKey::from_env_file_var(path, &args.api_key_env).map(Some),
        None => Ok(ApiKey::from_env_var(&args.api_key_env)),
    }
}

fn grammar_tools() -> Vec<Value> {
    let agent = Agent::new(
        Arc::new(BillsHost::new()),
        Arc::new(polyui_server::ScriptedProvider::new()),
    )
    .with_config(AgentConfig {
        tool_profile: ToolProfile::Full,
        ..AgentConfig::default()
    });
    let (session, _) = agent.open_session(run::hello("en"), User::new(HOUSEHOLD_USER));
    session.tools().to_vec()
}

fn providers(args: &Args) -> Result<Providers, String> {
    let wants_constrained = args.variant.contains(&Variant::Constrained);
    match args.provider {
        ProviderChoice::Anthropic => {
            if wants_constrained {
                return Err("the constrained setting needs --provider openai-compat".into());
            }
            let model = args
                .model
                .clone()
                .unwrap_or_else(|| anthropic::DEFAULT_MODEL.to_string());
            Ok(Providers {
                plain: Arc::new(AnthropicProvider::new(anthropic_key(args)?, model.clone())),
                constrained: None,
                compat: None,
                model,
            })
        }
        ProviderChoice::OpenaiCompat => {
            let base_url = args
                .base_url
                .as_deref()
                .ok_or("--provider openai-compat needs --base-url")?;
            let model = args
                .model
                .clone()
                .ok_or("--provider openai-compat needs --model")?;
            let mut compat = OpenAiCompatProvider::new(base_url, model.clone())
                .with_overloaded_attempts(args.server_error_attempts);
            if let Some(key) = compat_key(args)? {
                compat = compat.with_key(key);
            }
            if let Some(temperature) = args.temperature {
                compat = compat.with_temperature(temperature);
            }
            let constrained = if wants_constrained {
                let dialect = SchemaDialect::from(args.schema_dialect);
                let schema = run::constrained_schema(&grammar_tools(), dialect)?;
                eprintln!(
                    "constrained reply schema: {} bytes, {dialect} dialect",
                    schema.to_string().len()
                );
                let format = json_schema_format("polyui_reply", schema);
                Some(Arc::new(compat.clone().with_response_format(format)) as Arc<dyn ModelProvider>)
            } else {
                None
            };
            Ok(Providers {
                plain: Arc::new(compat.clone()),
                constrained,
                compat: Some(compat),
                model,
            })
        }
    }
}

fn jobs<'a>(
    bench: &'a Bench,
    prompts: &'a [Prompt],
    variants: &[Variant],
) -> Vec<BoxFuture<'a, RunRecord>> {
    let mut out: Vec<BoxFuture<'a, RunRecord>> = Vec::new();
    for variant in Variant::ALL
        .into_iter()
        .filter(|variant| variants.contains(variant))
    {
        for prompt in prompts {
            out.push(match variant {
                Variant::Tools => Box::pin(run::run_tools(bench, prompt)),
                Variant::Constrained => Box::pin(run::run_constrained(bench, prompt)),
                Variant::Inline => Box::pin(run::run_inline(bench, prompt)),
            });
        }
    }
    out
}

fn progress(record: &RunRecord) {
    let attempts: String = record
        .attempts
        .iter()
        .map(|attempt| if attempt.ok { 'v' } else { 'x' })
        .collect();
    eprintln!(
        "{:<22} {:<7} ui [{attempts}] opened {} calls {} {} ms, {}",
        record.prompt,
        record.variant.short(),
        record.opened,
        record.model_calls,
        record.ms_total,
        record.ending
    );
}

async fn run(args: Args) -> Result<(), String> {
    let mut prompts = prompts::prompts();
    if !args.only.is_empty() {
        prompts.retain(|prompt| args.only.contains(&prompt.id));
        if prompts.len() != args.only.len() {
            return Err("--only names a prompt id that does not exist; see --list".into());
        }
    }
    if let Some(limit) = args.limit {
        prompts.truncate(limit);
    }
    if args.list {
        for prompt in &prompts {
            println!(
                "{:<22} {:<8} {:<12} {}",
                prompt.id, prompt.locale, prompt.category, prompt.text
            );
        }
        return Ok(());
    }
    let unknown = tools::unknown_types(&BillsHost::new().catalogs(), &args.catalog_subset);
    if !unknown.is_empty() {
        return Err(format!(
            "--catalog-subset names types that are not in the catalogs: {}",
            unknown.join(", ")
        ));
    }
    let chosen = providers(&args)?;
    let bench = Bench {
        provider: chosen.plain.clone(),
        constrained: chosen.constrained.clone(),
        budget: Budget::new(args.max_calls, args.stop_after_errors),
        pacer: Pacer::new(Duration::from_millis(args.min_gap_ms)),
        config: AgentConfig {
            max_tokens: args.max_tokens,
            tool_profile: args.tool_profile.into(),
            ..AgentConfig::default()
        },
        tool_types: args.catalog_subset.clone(),
    };
    let mut records: Vec<RunRecord> =
        futures_util::stream::iter(jobs(&bench, &prompts, &args.variant))
            .buffer_unordered(args.concurrency.max(1))
            .inspect(progress)
            .collect()
            .await;
    records.sort_by(|a, b| {
        a.prompt
            .cmp(&b.prompt)
            .then_with(|| a.variant.cmp(&b.variant))
    });
    let date = today();
    let prices = Prices {
        input_per_million: args.input_price,
        output_per_million: args.output_price,
        cache_write_factor: args.cache_write_factor,
        cache_read_factor: args.cache_read_factor,
    };
    if bench.budget.stopped() {
        eprintln!("polyui-bench: stopped calling the model after provider errors in a row");
    }
    let calls = Calls {
        model_calls: bench.budget.used(),
        provider_requests: chosen
            .compat
            .as_ref()
            .map(OpenAiCompatProvider::requests_sent),
        stopped_early: bench.budget.stopped(),
    };
    let report = Report::build(
        date.clone(),
        chosen.model.clone(),
        ToolSetup {
            profile: bench.config.tool_profile.name(),
            catalog_subset: bench.tool_types.clone(),
        },
        prices,
        prompts.len(),
        calls,
        records,
    );
    let stem = args
        .name
        .clone()
        .unwrap_or_else(|| format!("{date}-{}", chosen.model.replace('/', "-")));
    std::fs::create_dir_all(&args.out_dir).map_err(|error| error.to_string())?;
    let json_path = args.out_dir.join(format!("{stem}.json"));
    let markdown_path = args.out_dir.join(format!("{stem}.md"));
    let json = serde_json::to_string_pretty(&report)
        .map_err(|error| error.to_string())?
        .replace('\u{2014}', "\\u2014");
    std::fs::write(&json_path, json + "\n").map_err(|error| error.to_string())?;
    let markdown = report.markdown();
    std::fs::write(&markdown_path, &markdown).map_err(|error| error.to_string())?;
    println!("{markdown}");
    println!(
        "results: {} and {}",
        json_path.display(),
        markdown_path.display()
    );
    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    match run(Args::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("polyui-bench: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn grammars_always_use_the_full_profile() {
        let tools = grammar_tools();
        assert!(tools[0]["input_schema"].to_string().contains("anyOf"));
        let standard = run::constrained_schema(&tools, SchemaDialect::Standard).unwrap();
        let merged = run::constrained_schema(&tools, SchemaDialect::NoSameTypeUnions).unwrap();
        assert!(!polyui_decode::same_type_unions(&standard).is_empty());
        assert!(polyui_decode::same_type_unions(&merged).is_empty());
        assert_eq!(
            merged["properties"]["tool"],
            serde_json::json!({ "type": "string", "enum": ["show_ui", "update_ui", "close_ui", "get_usage", "none"] })
        );
    }

    #[test]
    fn civil_dates() {
        assert_eq!(civil_date(0), (1970, 1, 1));
        assert_eq!(civil_date(20_781), (2026, 11, 24));
        assert_eq!(civil_date(11_016), (2000, 2, 29));
    }
}
