use clap::{Args, Parser, Subcommand, ValueEnum};
use polyui_decode::llguidance::api::TopLevelGrammar;
use polyui_decode::request::{self, Chat};
use polyui_decode::{CallFormat, Checker, Layout, Reply, ToolGrammar};
use serde_json::{Value, json};
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Debug, Parser)]
#[command(
    name = "polyui-decode",
    version,
    about = "Build llguidance grammars and request bodies that keep a local model's PolyUI tool calls valid."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Args)]
struct GrammarArgs {
    #[arg(
        long,
        value_name = "FILE",
        help = "Tool definitions as printed by `polyui tools`, or - for stdin."
    )]
    tools: PathBuf,
    #[arg(
        long,
        value_delimiter = ',',
        help = "Keep only these tools, for example show_ui,update_ui,close_ui."
    )]
    only: Vec<String>,
    #[arg(
        long,
        value_enum,
        default_value = "hermes",
        help = "How a tool call is wrapped."
    )]
    format: FormatChoice,
    #[arg(
        long,
        value_enum,
        default_value = "spaced",
        help = "Whitespace allowed in the JSON."
    )]
    layout: LayoutChoice,
    #[arg(long, help = "Force a tool call: plain text is not allowed.")]
    tool_only: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum FormatChoice {
    Hermes,
    HermesTokens,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum LayoutChoice {
    Spaced,
    Compact,
    Flexible,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Target {
    LlamaCompletion,
    LlamaChat,
    LlamaJson,
    VllmChat,
    VllmJson,
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(about = "Print the Lark grammar (starts with %llguidance).")]
    Grammar(GrammarArgs),
    #[command(about = "Print the llguidance-ready JSON Schema of one tool's input.")]
    Schema {
        #[command(flatten)]
        grammar: GrammarArgs,
        #[arg(long, default_value = "show_ui")]
        tool: String,
    },
    #[command(about = "Print the system prompt text that lists the tools and the call format.")]
    Prompt(GrammarArgs),
    #[command(about = "Print a request body for llama-server or vLLM.")]
    Body {
        #[arg(value_enum)]
        target: Target,
        #[command(flatten)]
        grammar: GrammarArgs,
        #[arg(long, help = "The user message.")]
        user: String,
        #[arg(long, help = "Extra system prompt text, put before the tool list.")]
        system: Option<String>,
        #[arg(long, help = "Model name, needed by vLLM.")]
        model: Option<String>,
        #[arg(long, default_value_t = 2048)]
        max_tokens: u32,
        #[arg(long, help = "Sampling temperature.")]
        temperature: Option<f64>,
        #[arg(
            long,
            default_value = "show_ui",
            help = "Tool whose input the JSON targets produce."
        )]
        tool: String,
        #[arg(
            long,
            help = "Prompt text for llama-completion, already in the model's chat template."
        )]
        prompt: Option<String>,
    },
    #[command(about = "Read a model reply and print the tool call or the text as JSON.")]
    Parse {
        #[command(flatten)]
        grammar: GrammarArgs,
        #[arg(long, value_name = "FILE", help = "Reply text; stdin when left out.")]
        reply: Option<PathBuf>,
        #[arg(long, help = "Print only the arguments of a tool call.")]
        arguments: bool,
    },
    #[command(about = "Print the generated text from a llama-server or vLLM response body.")]
    Content {
        #[arg(
            long,
            value_name = "FILE",
            help = "Response body; stdin when left out."
        )]
        response: Option<PathBuf>,
    },
    #[command(about = "Compile the grammar and every tool schema in llguidance and time it.")]
    Check(GrammarArgs),
}

fn read_input(path: &PathBuf) -> Result<String, String> {
    if path.as_os_str() == "-" {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|error| format!("cannot read stdin: {error}"))?;
        Ok(text)
    } else {
        std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))
    }
}

fn load(args: &GrammarArgs) -> Result<ToolGrammar, String> {
    let text = read_input(&args.tools)?;
    let definitions: Value = serde_json::from_str(&text)
        .map_err(|error| format!("tool definitions are not JSON: {error}"))?;
    let mut grammar = ToolGrammar::from_tools(&definitions).map_err(|error| error.to_string())?;
    if !args.only.is_empty() {
        let names: Vec<&str> = args.only.iter().map(String::as_str).collect();
        grammar = grammar.only(&names).map_err(|error| error.to_string())?;
    }
    let mut format = match args.format {
        FormatChoice::Hermes => CallFormat::hermes(),
        FormatChoice::HermesTokens => CallFormat::hermes_tokens(),
    };
    if args.tool_only {
        format = format.tool_only();
    }
    let layout = match args.layout {
        LayoutChoice::Spaced => Layout::Spaced,
        LayoutChoice::Compact => Layout::Compact,
        LayoutChoice::Flexible => Layout::Flexible,
    };
    for (tool, change) in grammar.changes() {
        eprintln!("note: {tool} {change}");
    }
    Ok(grammar.with_format(format).with_layout(layout))
}

fn print_json(value: &Value) -> Result<(), String> {
    serde_json::to_string_pretty(value)
        .map(|text| println!("{text}"))
        .map_err(|error| error.to_string())
}

fn body(
    target: Target,
    grammar: &ToolGrammar,
    chat: Chat,
    tool: &str,
    prompt: Option<&str>,
) -> Result<Value, String> {
    let missing = || format!("no tool named {tool} in the grammar");
    match target {
        Target::LlamaCompletion => {
            let prompt =
                prompt.ok_or("llama-completion needs --prompt in the model's chat template")?;
            Ok(request::llama_completion(grammar, prompt, chat.max_tokens))
        }
        Target::LlamaChat => Ok(request::llama_chat_grammar(
            grammar,
            &chat.with_tools_prompt(grammar),
        )),
        Target::VllmChat => Ok(request::vllm_chat_grammar(
            grammar,
            &chat.with_tools_prompt(grammar),
        )),
        Target::LlamaJson | Target::VllmJson => {
            let system = grammar.json_prompt(tool).ok_or_else(missing)?;
            let chat = chat.with_system(&system);
            let made = if matches!(target, Target::LlamaJson) {
                request::llama_chat_json(grammar, tool, &chat)
            } else {
                request::vllm_chat_json(grammar, tool, &chat)
            };
            made.ok_or_else(missing)
        }
    }
}

fn generated_text(body: &Value) -> Option<&str> {
    let choice = body.get("choices").and_then(|choices| choices.get(0));
    choice
        .and_then(|choice| choice.pointer("/message/content"))
        .or_else(|| choice.and_then(|choice| choice.get("text")))
        .or_else(|| body.get("content"))
        .and_then(Value::as_str)
}

fn check(grammar: &ToolGrammar) -> Result<(), String> {
    let checker = Checker::byte_level();
    let lark = grammar.lark();
    let timing = checker.time(TopLevelGrammar::from_lark(lark.clone()))?;
    println!(
        "grammar: {} bytes, compile {:.1} ms, first mask {:.2} ms, {} warnings",
        lark.len(),
        timing.compile.as_secs_f64() * 1000.0,
        timing.first_mask.as_secs_f64() * 1000.0,
        timing.warnings.len()
    );
    for tool in grammar.tools() {
        let timing = checker.time(TopLevelGrammar::from_json_schema(tool.input_schema.clone()))?;
        println!(
            "{}: compile {:.1} ms, first mask {:.2} ms, {} warnings",
            tool.name,
            timing.compile.as_secs_f64() * 1000.0,
            timing.first_mask.as_secs_f64() * 1000.0,
            timing.warnings.len()
        );
    }
    Ok(())
}

fn run(command: Command) -> Result<ExitCode, String> {
    match command {
        Command::Grammar(args) => {
            print!("{}", load(&args)?.lark());
        }
        Command::Schema { grammar, tool } => {
            let grammar = load(&grammar)?;
            let schema = grammar
                .input_schema(&tool)
                .ok_or_else(|| format!("no tool named {tool} in the grammar"))?;
            print_json(schema)?;
        }
        Command::Prompt(args) => {
            println!("{}", load(&args)?.tools_prompt());
        }
        Command::Body {
            target,
            grammar,
            user,
            system,
            model,
            max_tokens,
            temperature,
            tool,
            prompt,
        } => {
            let grammar = load(&grammar)?;
            let mut chat = Chat::new(vec![json!({ "role": "user", "content": user })])
                .with_max_tokens(max_tokens);
            if let Some(system) = system {
                chat = chat.with_system(&system);
            }
            if let Some(model) = model {
                chat = chat.with_model(model);
            }
            if let Some(temperature) = temperature {
                chat = chat.with_temperature(temperature);
            }
            print_json(&body(target, &grammar, chat, &tool, prompt.as_deref())?)?;
        }
        Command::Parse {
            grammar,
            reply,
            arguments,
        } => {
            let grammar = load(&grammar)?;
            let text = read_input(&reply.unwrap_or_else(|| PathBuf::from("-")))?;
            return match grammar.parse_reply(&text) {
                Reply::Call {
                    name,
                    arguments: args,
                } => {
                    if arguments {
                        print_json(&args)?;
                    } else {
                        print_json(&json!({ "name": name, "arguments": args }))?;
                    }
                    Ok(ExitCode::SUCCESS)
                }
                Reply::Text(text) => {
                    print_json(&json!({ "text": text }))?;
                    Ok(ExitCode::from(3))
                }
                Reply::Invalid(problem) => Err(problem),
            };
        }
        Command::Content { response } => {
            let text = read_input(&response.unwrap_or_else(|| PathBuf::from("-")))?;
            let body: Value = serde_json::from_str(&text)
                .map_err(|error| format!("the response is not JSON: {error}"))?;
            let content = generated_text(&body)
                .ok_or_else(|| format!("no generated text in the response: {}", body))?;
            print!("{content}");
        }
        Command::Check(args) => check(&load(&args)?)?,
    }
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    match run(Cli::parse().command) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
