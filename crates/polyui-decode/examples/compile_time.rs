use polyui_core::{ActionSpec, ActionTable, CatalogSet};
use polyui_decode::llguidance::api::TopLevelGrammar;
use polyui_decode::llguidance::toktrie::{ApproximateTokEnv, TokEnv, TokRxInfo, TokTrie};
use polyui_decode::{Checker, ToolGrammar};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

const RUNS: usize = 20;
const WORD_LIST: &str = "/usr/share/dict/words";
const SYNTHETIC_VOCAB: usize = 150_000;

fn actions() -> ActionTable {
    let pay = ActionSpec::from_value(&json!({
        "args": { "bill": { "kind": "string", "required": true, "max": 32 } },
        "requires_confirm": true
    }))
    .expect("action spec parses");
    ActionTable::new()
        .with("pay_bill", pay)
        .with("pick_store", ActionSpec::new(false))
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn stats(mut samples: Vec<Duration>) -> String {
    samples.sort();
    format!(
        "median {:7.2} ms  min {:7.2} ms  max {:7.2} ms",
        milliseconds(samples[samples.len() / 2]),
        milliseconds(samples[0]),
        milliseconds(samples[samples.len() - 1])
    )
}

fn measure(checker: &Checker, label: &str, grammar: &TopLevelGrammar, size: usize) {
    let mut compile = Vec::with_capacity(RUNS);
    let mut mask = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let timing = checker.time(grammar.clone()).expect("grammar compiles");
        compile.push(timing.compile);
        mask.push(timing.first_mask);
    }
    println!("  {label:<34} {size:>7} bytes  compile {}", stats(compile));
    println!("  {:<34} {:>13}  first mask {}", "", "", stats(mask));
}

fn synthetic_env() -> Option<TokEnv> {
    let text = std::fs::read_to_string(WORD_LIST).ok()?;
    let mut words: Vec<Vec<u8>> = (0..=255u8).map(|byte| vec![byte]).collect();
    let mut seen = BTreeSet::new();
    for punctuation in [
        "{\"",
        "\":",
        "\": ",
        "\", \"",
        "\"}",
        "}}",
        "]}",
        "}]",
        "\"]",
        "[\"",
        ", ",
        "\n",
        "<tool_call>",
        "</tool_call>",
        "_ui",
        "show",
        "props",
        "type",
        "children",
        "label",
    ] {
        seen.insert(punctuation.as_bytes().to_vec());
    }
    for word in text.lines().map(str::trim).filter(|word| word.len() > 1) {
        if seen.len() + words.len() >= SYNTHETIC_VOCAB {
            break;
        }
        seen.insert(word.as_bytes().to_vec());
        seen.insert(format!(" {word}").into_bytes());
    }
    words.extend(seen);
    words.push(b"\xFF<|endoftext|>".to_vec());
    let info = TokRxInfo::new(words.len() as u32, words.len() as u32 - 1);
    Some(Arc::new(ApproximateTokEnv::new(TokTrie::from(
        &info, &words,
    ))))
}

fn per_token_masks(checker: &Checker, lark: &str, text: &str) -> (usize, Duration, Duration) {
    let mut matcher = checker.lark(lark).expect("grammar compiles");
    let tokens = checker.tok_env().tokenize_bytes(text.as_bytes());
    let mut total = Duration::ZERO;
    let mut worst = Duration::ZERO;
    for token in &tokens {
        let started = Instant::now();
        matcher.compute_mask().expect("mask");
        let spent = started.elapsed();
        total += spent;
        worst = worst.max(spent);
        matcher.consume_token(*token).expect("token is allowed");
    }
    (tokens.len(), total / tokens.len() as u32, worst)
}

fn report(checker: &Checker, vocabulary: &str) {
    println!(
        "\ntokenizer: {vocabulary}, {} tokens",
        checker.tok_env().tok_trie().vocab_size()
    );
    let catalogs = CatalogSet::builtin();
    let actions = actions();
    let all = ToolGrammar::new(&catalogs, &actions).expect("builds");
    let core =
        ToolGrammar::for_client(&catalogs, &["core@1".to_string()], &actions).expect("builds");
    let show = all.clone().only(&["show_ui"]).expect("show_ui is there");
    for (label, grammar) in [
        ("lark: show, update, close", &all),
        ("lark: show_ui only", &show),
        ("lark: core@1 client, three tools", &core),
    ] {
        let lark = grammar.lark();
        measure(
            checker,
            label,
            &TopLevelGrammar::from_lark(lark.clone()),
            lark.len(),
        );
    }
    for tool in all.tools() {
        let size = tool.input_schema.to_string().len();
        measure(
            checker,
            &format!("json schema: {}", tool.name),
            &TopLevelGrammar::from_json_schema(tool.input_schema.clone()),
            size,
        );
    }
    let input: Value = json!({
        "surface": "s1",
        "root": "card",
        "components": [
            { "id": "card", "type": "Card", "props": { "title": "Your bills" }, "children": ["list"] },
            { "id": "list", "type": "List", "children": ["gas", "power"] },
            { "id": "gas", "type": "Button", "props": { "label": "Gas 29 322 so'm", "tone": "primary", "on_press": { "action": "pay_bill", "args": { "bill": "b_17" } } } },
            { "id": "power", "type": "Button", "props": { "label": { "$ref": "bill:power" }, "on_press": { "action": "pay_bill", "args": { "bill": "b_18" } } } }
        ]
    });
    let call = all.render_call("show_ui", &input);
    let (count, mean, worst) = per_token_masks(checker, &all.lark(), &call);
    println!(
        "  masks while writing the bills card call: {count} tokens, mean {:.3} ms, worst {:.3} ms",
        milliseconds(mean),
        milliseconds(worst)
    );
}

fn main() {
    report(&Checker::byte_level(), "byte level");
    match synthetic_env() {
        Some(env) => {
            let started = Instant::now();
            let checker = Checker::new(&env).expect("factory builds");
            println!(
                "\nparser factory for the synthetic vocabulary: {:.1} ms (once per model)",
                milliseconds(started.elapsed())
            );
            report(&checker, "synthetic, words from /usr/share/dict/words");
        }
        None => println!("\n{WORD_LIST} is missing, skipped the large vocabulary run"),
    }
}
