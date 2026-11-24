mod meter;
mod prompts;
mod report;
mod run;

use clap::{Parser, ValueEnum};
use futures_util::StreamExt;
use futures_util::future::BoxFuture;
use meter::Budget;
use polyui_server::anthropic::{self, AnthropicProvider, ApiKey};
use prompts::Prompt;
use report::{Prices, Report};
use run::{Bench, RunRecord, Variant};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Which {
    Both,
    Tools,
    Inline,
}

#[derive(Debug, Parser)]
#[command(
    name = "polyui-bench",
    about = "Measures how often the model produces a valid show_ui on the first try and after retries."
)]
struct Args {
    #[arg(long, default_value = anthropic::DEFAULT_MODEL)]
    model: String,
    #[arg(
        long,
        help = "File with ANTHROPIC_API_KEY=...; without it the key comes from the environment"
    )]
    env_file: Option<PathBuf>,
    #[arg(long, value_enum, default_value = "both")]
    variant: Which,
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

fn api_key(args: &Args) -> Result<ApiKey, String> {
    match &args.env_file {
        Some(path) => ApiKey::from_env_file(path),
        None => ApiKey::from_env().ok_or_else(|| format!("{} is not set", anthropic::KEY_VARIABLE)),
    }
}

fn jobs<'a>(
    bench: &'a Bench,
    prompts: &'a [Prompt],
    which: Which,
) -> Vec<BoxFuture<'a, RunRecord>> {
    let mut out: Vec<BoxFuture<'a, RunRecord>> = Vec::new();
    for prompt in prompts {
        if matches!(which, Which::Both | Which::Tools) {
            out.push(Box::pin(run::run_tools(bench, prompt)));
        }
        if matches!(which, Which::Both | Which::Inline) {
            out.push(Box::pin(run::run_inline(bench, prompt)));
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
        match record.variant {
            Variant::Tools => "tools",
            Variant::Inline => "inline",
        },
        record.opened,
        record.model_calls,
        record.ms_total,
        record.ending
    );
}

async fn run(args: Args) -> Result<(), String> {
    let mut prompts = prompts::prompts();
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
    let key = api_key(&args)?;
    let provider = Arc::new(AnthropicProvider::new(key, args.model.clone()));
    let bench = Bench {
        provider,
        budget: Budget::new(args.max_calls),
    };
    let mut records: Vec<RunRecord> =
        futures_util::stream::iter(jobs(&bench, &prompts, args.variant))
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
    let report = Report::build(
        date.clone(),
        args.model.clone(),
        prices,
        prompts.len(),
        bench.budget.used(),
        records,
    );
    let stem = args
        .name
        .clone()
        .unwrap_or_else(|| format!("{date}-{}", args.model));
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

    #[test]
    fn civil_dates() {
        assert_eq!(civil_date(0), (1970, 1, 1));
        assert_eq!(civil_date(20_781), (2026, 11, 24));
        assert_eq!(civil_date(11_016), (2000, 2, 29));
    }
}
