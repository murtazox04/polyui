use crate::prompts::{Expect, LOCALES};
use crate::run::{RunRecord, Variant};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt::Write;

const KIND_CHARS: usize = 110;
const TOP_KINDS: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Prices {
    pub input_per_million: f64,
    pub output_per_million: f64,
    pub cache_write_factor: f64,
    pub cache_read_factor: f64,
}

impl Prices {
    pub fn cost(&self, record: &RunRecord) -> f64 {
        let input = record.input_tokens as f64
            + record.cache_write_tokens as f64 * self.cache_write_factor
            + record.cache_read_tokens as f64 * self.cache_read_factor;
        (input * self.input_per_million + record.output_tokens as f64 * self.output_per_million)
            / 1_000_000.0
    }
}

fn normalize(message: &str) -> String {
    let first = message.split(". ").next().unwrap_or(message);
    let mut out = String::new();
    let mut quoted = false;
    let mut digits = false;
    for c in first.chars() {
        if c == '"' {
            if !quoted {
                out.push_str("\"x\"");
            }
            quoted = !quoted;
            digits = false;
            continue;
        }
        if quoted {
            continue;
        }
        if c.is_ascii_digit() {
            if !digits {
                out.push('N');
            }
            digits = true;
            continue;
        }
        digits = false;
        out.push(c);
    }
    let trimmed = out.trim().trim_end_matches('.').to_string();
    trimmed.chars().take(KIND_CHARS).collect()
}

fn numbered(line: &str) -> Option<&str> {
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    (digits > 0)
        .then(|| line[digits..].strip_prefix(". "))
        .flatten()
}

pub fn problem_kinds(content: &str) -> Vec<String> {
    if content.contains("INVALID_JSON") {
        return vec!["the input is not valid JSON".to_string()];
    }
    let kinds: Vec<String> = content
        .lines()
        .filter_map(numbered)
        .map(|line| match line.split_once(": ") {
            Some((_, message)) => normalize(message),
            None => normalize(line),
        })
        .collect();
    if kinds.is_empty() {
        vec![normalize(content.lines().next().unwrap_or_default())]
    } else {
        kinds
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Summary {
    pub runs: usize,
    pub ui_runs: usize,
    pub first_try_valid: usize,
    pub valid_after_one_retry: usize,
    pub valid_after_two_retries: usize,
    pub ui_expected: usize,
    pub ui_shown_when_expected: usize,
    pub no_ui_expected: usize,
    pub no_ui_when_not_expected: usize,
    pub mean_ms_to_open: Option<f64>,
    pub model_calls: usize,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_write_tokens: u64,
    pub cache_read_tokens: u64,
    pub cost_usd: f64,
    pub budget_cut: usize,
    pub problem_kinds: Vec<(String, usize)>,
}

impl Summary {
    pub fn of(records: &[&RunRecord], prices: &Prices) -> Self {
        let ui: Vec<&&RunRecord> = records.iter().filter(|run| run.made_ui_call()).collect();
        let within = |limit: usize| {
            ui.iter()
                .filter(|run| run.first_valid().is_some_and(|index| index <= limit))
                .count()
        };
        let expected: Vec<&&RunRecord> = records
            .iter()
            .filter(|run| run.expect == Expect::Ui)
            .collect();
        let quiet: Vec<&&RunRecord> = records
            .iter()
            .filter(|run| run.expect == Expect::NoUi)
            .collect();
        let opens: Vec<u64> = records.iter().filter_map(|run| run.ms_to_open).collect();
        let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
        for problem in records
            .iter()
            .flat_map(|run| run.attempts.iter())
            .flat_map(|attempt| attempt.problems.iter())
        {
            *kinds.entry(problem.clone()).or_default() += 1;
        }
        let mut problem_kinds: Vec<(String, usize)> = kinds.into_iter().collect();
        problem_kinds.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        problem_kinds.truncate(TOP_KINDS);
        Self {
            runs: records.len(),
            ui_runs: ui.len(),
            first_try_valid: within(0),
            valid_after_one_retry: within(1),
            valid_after_two_retries: within(2),
            ui_expected: expected.len(),
            ui_shown_when_expected: expected.iter().filter(|run| run.opened).count(),
            no_ui_expected: quiet.len(),
            no_ui_when_not_expected: quiet.iter().filter(|run| !run.made_ui_call()).count(),
            mean_ms_to_open: (!opens.is_empty())
                .then(|| opens.iter().sum::<u64>() as f64 / opens.len() as f64),
            model_calls: records.iter().map(|run| run.model_calls).sum(),
            input_tokens: records.iter().map(|run| run.input_tokens).sum(),
            output_tokens: records.iter().map(|run| run.output_tokens).sum(),
            cache_write_tokens: records.iter().map(|run| run.cache_write_tokens).sum(),
            cache_read_tokens: records.iter().map(|run| run.cache_read_tokens).sum(),
            cost_usd: records.iter().map(|run| prices.cost(run)).sum(),
            budget_cut: records.iter().filter(|run| run.budget_cut()).count(),
            problem_kinds,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VariantReport {
    pub variant: Variant,
    pub overall: Summary,
    pub per_locale: BTreeMap<String, Summary>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Report {
    pub date: String,
    pub model: String,
    pub tools: ToolSetup,
    pub prices: Prices,
    pub prompts: usize,
    pub total_model_calls: usize,
    pub provider_requests: Option<usize>,
    pub stopped_early: bool,
    pub total_cost_usd: f64,
    pub variants: Vec<VariantReport>,
    pub runs: Vec<RunRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolSetup {
    pub profile: &'static str,
    pub catalog_subset: Vec<String>,
}

impl ToolSetup {
    fn describe(&self) -> String {
        if self.catalog_subset.is_empty() {
            format!("the {} tool profile", self.profile)
        } else {
            format!(
                "the {} tool profile and only {}",
                self.profile,
                self.catalog_subset.join(", ")
            )
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Calls {
    pub model_calls: usize,
    pub provider_requests: Option<usize>,
    pub stopped_early: bool,
}

impl Report {
    pub fn build(
        date: String,
        model: String,
        tools: ToolSetup,
        prices: Prices,
        prompts: usize,
        calls: Calls,
        runs: Vec<RunRecord>,
    ) -> Self {
        let mut variants = Vec::new();
        for variant in Variant::ALL {
            let records: Vec<&RunRecord> =
                runs.iter().filter(|run| run.variant == variant).collect();
            if records.is_empty() {
                continue;
            }
            let per_locale = LOCALES
                .iter()
                .map(|locale| {
                    let local: Vec<&RunRecord> = records
                        .iter()
                        .copied()
                        .filter(|run| run.locale == *locale)
                        .collect();
                    (locale.to_string(), Summary::of(&local, &prices))
                })
                .filter(|(_, summary)| summary.runs > 0)
                .collect();
            variants.push(VariantReport {
                variant,
                overall: Summary::of(&records, &prices),
                per_locale,
            });
        }
        let total_cost_usd = runs.iter().map(|run| prices.cost(run)).sum();
        Self {
            date,
            model,
            tools,
            prices,
            prompts,
            total_model_calls: calls.model_calls,
            provider_requests: calls.provider_requests,
            stopped_early: calls.stopped_early,
            total_cost_usd,
            variants,
            runs,
        }
    }

    pub fn markdown(&self) -> String {
        let mut out = String::new();
        let _ = write!(
            out,
            "Run on {} with `{}` and {}: {} prompts, {} model calls, about ${:.4}.",
            self.date,
            self.model,
            self.tools.describe(),
            self.prompts,
            self.total_model_calls,
            self.total_cost_usd
        );
        if let Some(requests) = self.provider_requests {
            let _ = write!(out, " {requests} HTTP requests including retries.");
        }
        if self.stopped_early {
            out.push_str(" The run stopped early after provider errors in a row.");
        }
        out.push_str("\n\n");
        out.push_str("| Setting | Turns with a UI call | Valid on the first try | Valid after 1 retry | Valid after 2 retries | UI shown when expected | No UI when none expected | Mean time to ui.open | Model calls | Input tokens | Output tokens | Cost |\n");
        out.push_str("|---|---|---|---|---|---|---|---|---|---|---|---|\n");
        for variant in &self.variants {
            let summary = &variant.overall;
            let _ = writeln!(
                out,
                "| {} | {} of {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | ${:.4} |",
                variant.variant.label(),
                summary.ui_runs,
                summary.runs,
                rate(summary.first_try_valid, summary.ui_runs),
                rate(summary.valid_after_one_retry, summary.ui_runs),
                rate(summary.valid_after_two_retries, summary.ui_runs),
                rate(summary.ui_shown_when_expected, summary.ui_expected),
                rate(summary.no_ui_when_not_expected, summary.no_ui_expected),
                seconds(summary.mean_ms_to_open),
                summary.model_calls,
                input_total(summary),
                summary.output_tokens,
                summary.cost_usd
            );
        }
        out.push_str("\n| Locale | Setting | Turns with a UI call | Valid on the first try | Valid after 2 retries | Mean time to ui.open |\n");
        out.push_str("|---|---|---|---|---|---|\n");
        for locale in LOCALES {
            for variant in &self.variants {
                if let Some(summary) = variant.per_locale.get(locale) {
                    let _ = writeln!(
                        out,
                        "| {locale} | {} | {} of {} | {} | {} | {} |",
                        variant.variant.label(),
                        summary.ui_runs,
                        summary.runs,
                        rate(summary.first_try_valid, summary.ui_runs),
                        rate(summary.valid_after_two_retries, summary.ui_runs),
                        seconds(summary.mean_ms_to_open)
                    );
                }
            }
        }
        out.push_str("\n| Setting | Most common problems | Count |\n|---|---|---|\n");
        for variant in &self.variants {
            if variant.overall.problem_kinds.is_empty() {
                let _ = writeln!(out, "| {} | none | 0 |", variant.variant.label());
            }
            for (kind, count) in &variant.overall.problem_kinds {
                let _ = writeln!(
                    out,
                    "| {} | {} | {count} |",
                    variant.variant.label(),
                    kind.replace('|', "/")
                );
            }
        }
        out
    }
}

fn input_total(summary: &Summary) -> u64 {
    summary.input_tokens + summary.cache_write_tokens + summary.cache_read_tokens
}

pub fn rate(part: usize, whole: usize) -> String {
    if whole == 0 {
        "n/a".to_string()
    } else {
        format!(
            "{:.0}% ({part}/{whole})",
            part as f64 * 100.0 / whole as f64
        )
    }
}

fn seconds(ms: Option<f64>) -> String {
    ms.map_or_else(|| "n/a".to_string(), |ms| format!("{:.1} s", ms / 1000.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::Attempt;

    fn run(attempts: &[bool], expect: Expect) -> RunRecord {
        RunRecord {
            prompt: "en-bills".into(),
            locale: "en",
            category: "bills",
            expect,
            variant: Variant::Tools,
            attempts: attempts
                .iter()
                .map(|ok| Attempt {
                    tool: "show_ui".into(),
                    ok: *ok,
                    problems: if *ok { Vec::new() } else { vec!["kind".into()] },
                    call: String::new(),
                })
                .collect(),
            opened: attempts.contains(&true),
            ms_to_open: attempts.contains(&true).then_some(2000),
            ms_total: 3000,
            model_calls: attempts.len() + 1,
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            cache_write_tokens: 0,
            cache_read_tokens: 1_000_000,
            ending: "done".into(),
            text: String::new(),
        }
    }

    fn prices() -> Prices {
        Prices {
            input_per_million: 0.10,
            output_per_million: 0.50,
            cache_write_factor: 1.25,
            cache_read_factor: 0.1,
        }
    }

    #[test]
    fn problem_lines_become_kinds() {
        let text = "2 problems, nothing was shown:\n1. components[3] id \"pay\": type \"PayButton\" is not in the catalog. Known: Button, Card\n2. components[1] id \"list\" prop \"title\": text is 130 characters, max is 120.";
        assert_eq!(
            problem_kinds(text),
            vec![
                "type \"x\" is not in the catalog".to_string(),
                "text is N characters, max is N".to_string()
            ]
        );
        assert_eq!(
            problem_kinds("{\"INVALID_JSON\": \"{\"}"),
            vec!["the input is not valid JSON".to_string()]
        );
        assert_eq!(
            problem_kinds("unknown tool \"pay\""),
            vec!["unknown tool \"x\"".to_string()]
        );
    }

    #[test]
    fn rates_count_retries_within_the_limit() {
        let records = [
            run(&[true], Expect::Ui),
            run(&[false, true], Expect::Ui),
            run(&[false, false, true], Expect::Ui),
            run(&[false, false, false], Expect::Ui),
            run(&[], Expect::NoUi),
        ];
        let refs: Vec<&RunRecord> = records.iter().collect();
        let summary = Summary::of(&refs, &prices());
        assert_eq!(summary.ui_runs, 4);
        assert_eq!(summary.first_try_valid, 1);
        assert_eq!(summary.valid_after_one_retry, 2);
        assert_eq!(summary.valid_after_two_retries, 3);
        assert_eq!(summary.ui_shown_when_expected, 3);
        assert_eq!(summary.no_ui_when_not_expected, 1);
        assert_eq!(summary.problem_kinds, vec![("kind".to_string(), 6)]);
        assert_eq!(summary.mean_ms_to_open, Some(2000.0));
    }

    #[test]
    fn cost_uses_cache_factors() {
        let record = run(&[true], Expect::Ui);
        let cost = prices().cost(&record);
        assert!((cost - (0.10 + 0.01 + 0.50)).abs() < 1e-9, "{cost}");
    }

    #[test]
    fn rate_text() {
        assert_eq!(rate(3, 4), "75% (3/4)");
        assert_eq!(rate(0, 0), "n/a");
    }
}
