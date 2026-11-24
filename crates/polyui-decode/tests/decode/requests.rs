use crate::support::case;
use polyui_decode::request::{
    Chat, llama_chat_grammar, llama_chat_json, llama_completion, vllm_chat_grammar, vllm_chat_json,
};
use polyui_decode::{Checker, ToolGrammar};
use serde_json::{Value, json};

fn grammar() -> ToolGrammar {
    case("ok_bills_card.json").grammar()
}

fn chat() -> Chat {
    Chat::new(vec![
        json!({ "role": "system", "content": "You help the Karimov household with bills." }),
        json!({ "role": "user", "content": "Show my bills" }),
    ])
    .with_max_tokens(1024)
}

fn assert_grammar_compiles(text: &Value) {
    let text = text.as_str().expect("grammar is a string");
    assert!(
        text.starts_with("%llguidance"),
        "llama.cpp routes on this prefix"
    );
    assert!(Checker::byte_level().lark(text).is_ok());
}

fn assert_schema_compiles(schema: &Value) {
    assert!(Checker::byte_level().json_schema(schema).is_ok());
}

#[test]
fn llama_completion_carries_the_grammar_and_raw_prompt() {
    let grammar = grammar();
    let body = llama_completion(&grammar, "<|im_start|>user\nhi<|im_end|>\n", 512);
    assert_eq!(body["n_predict"], json!(512));
    assert_eq!(body["prompt"], json!("<|im_start|>user\nhi<|im_end|>\n"));
    assert_eq!(body["grammar"], json!(grammar.lark()));
    assert_grammar_compiles(&body["grammar"]);
}

#[test]
fn llama_chat_puts_the_tools_in_the_system_message_and_sends_no_tools_field() {
    let grammar = grammar();
    let body = llama_chat_grammar(&grammar, &chat().with_tools_prompt(&grammar));
    assert!(
        body.get("tools").is_none(),
        "llama-server refuses grammar together with tools"
    );
    assert_eq!(body["max_tokens"], json!(1024));
    let messages = body["messages"].as_array().expect("messages");
    assert_eq!(messages.len(), 2);
    let system = messages[0]["content"].as_str().expect("system text");
    assert!(
        system.starts_with("You help the Karimov household with bills.\n\nYou can call one tool")
    );
    assert!(system.contains("<tools>"));
    assert_eq!(messages[1]["content"], json!("Show my bills"));
    assert_grammar_compiles(&body["grammar"]);
}

#[test]
fn json_schema_bodies_use_the_tool_input_schema() {
    let grammar = grammar();
    let llama = llama_chat_json(&grammar, "show_ui", &chat()).expect("show_ui is there");
    assert_eq!(llama["response_format"]["type"], json!("json_schema"));
    assert_eq!(
        llama["response_format"]["json_schema"]["name"],
        json!("show_ui")
    );
    let schema = &llama["response_format"]["json_schema"]["schema"];
    assert_eq!(schema, grammar.input_schema("show_ui").expect("schema"));
    assert_schema_compiles(schema);
    let vllm = vllm_chat_json(&grammar, "show_ui", &chat().with_model("Qwen/Qwen3-8B"))
        .expect("show_ui is there");
    assert_eq!(vllm["model"], json!("Qwen/Qwen3-8B"));
    assert_eq!(&vllm["structured_outputs"]["json"], schema);
    assert!(llama_chat_json(&grammar, "nope", &chat()).is_none());
}

#[test]
fn vllm_chat_sends_the_same_lark_grammar() {
    let grammar = grammar();
    let body = vllm_chat_grammar(
        &grammar,
        &chat()
            .with_model("Qwen/Qwen3-8B")
            .with_temperature(0.2)
            .with_tools_prompt(&grammar),
    );
    assert_eq!(body["model"], json!("Qwen/Qwen3-8B"));
    assert_eq!(body["temperature"], json!(0.2));
    assert!(body.get("guided_grammar").is_none() && body.get("guided_decoding_backend").is_none());
    assert_eq!(body["structured_outputs"]["grammar"], json!(grammar.lark()));
    assert_grammar_compiles(&body["structured_outputs"]["grammar"]);
}

#[test]
fn a_system_message_is_added_when_there_is_none() {
    let grammar = grammar();
    let chat =
        Chat::new(vec![json!({ "role": "user", "content": "hi" })]).with_tools_prompt(&grammar);
    assert_eq!(chat.messages[0]["role"], json!("system"));
    assert_eq!(chat.messages[0]["content"], json!(grammar.tools_prompt()));
    assert_eq!(chat.messages.len(), 2);
}
