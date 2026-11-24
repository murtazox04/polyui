use polyui_core::client::{MAX_SCENE_ITEMS, MAX_SCENE_STRING_CHARS};
use polyui_core::{Client, RenderBatch};
use serde_json::{Value, json};

fn client() -> Client {
    let hello = json!({ "v": 1, "t": "hello", "catalogs": ["core@1", "chart@1"], "modalities": ["screen"], "locale": "en" });
    Client::new(&hello.to_string()).unwrap_or_else(|error| panic!("{error}"))
}

fn line_scene(points: usize, description: &str) -> Value {
    let commands: Vec<Value> = (0..points)
        .map(|index| json!({ "op": if index == 0 { "M" } else { "L" }, "x": index % 300, "y": index % 7 }))
        .collect();
    json!({
        "view_box": { "x": 0, "y": 0, "width": 320, "height": 200 },
        "kind": "line",
        "title": "Readings",
        "description": description,
        "marks": [{ "type": "path", "commands": commands, "stroke": "series-1", "stroke_width": 2 }],
        "legend": [],
        "hits": [],
        "skipped": 0
    })
}

fn chart(id: &str, scene: Value) -> Value {
    json!({ "id": id, "type": "chart:Chart", "props": { "title": "Readings", "kind": "line", "rows": [{ "x": 1, "y": 2 }], "scene": scene } })
}

fn open(components: Value, data: Value) -> String {
    json!({ "v": 1, "t": "ui.open", "surface": "s1", "seq": 1, "root": "c", "components": components, "data": data }).to_string()
}

fn rejection(batch: &RenderBatch) -> String {
    assert!(batch.ops.is_empty(), "rejected messages draw nothing");
    let error = batch
        .send
        .first()
        .unwrap_or_else(|| panic!("expected a ui.error"));
    assert_eq!(error["reason"], json!("bad_message"));
    error["detail"].as_str().unwrap_or_default().to_string()
}

#[test]
fn chart_scene_may_exceed_the_prop_limits() {
    let mut client = client();
    let long = "a".repeat(MAX_SCENE_STRING_CHARS);
    let message = open(json!([chart("c", line_scene(8000, &long))]), json!({}));
    assert!(message.len() < 256 * 1024);
    let batch = client.receive(&message);
    assert!(batch.send.is_empty(), "unexpected {:?}", batch.send);
    let scene = batch
        .to_value()
        .pointer("/ops/0/props/scene")
        .cloned()
        .unwrap_or(Value::Null);
    assert_eq!(
        scene["marks"][0]["commands"].as_array().map(Vec::len),
        Some(8000)
    );
    assert_eq!(
        scene["description"].as_str().map(str::len),
        Some(MAX_SCENE_STRING_CHARS)
    );
}

#[test]
fn patched_charts_get_the_same_room() {
    let mut client = client();
    let first = client.receive(&open(json!([chart("c", line_scene(10, "x"))]), json!({})));
    assert!(first.send.is_empty());
    let patch = json!({ "v": 1, "t": "ui.patch", "surface": "s1", "seq": 2, "upsert": [chart("c", line_scene(5000, "y"))] });
    let batch = client.receive(&patch.to_string());
    assert!(batch.send.is_empty(), "unexpected {:?}", batch.send);
    assert_eq!(batch.ops.len(), 1);
}

#[test]
fn scene_limits_still_apply() {
    let mut client = client();
    let zeros = vec![0; MAX_SCENE_ITEMS + 1];
    let too_many = open(json!([chart("c", json!({ "marks": zeros }))]), json!({}));
    assert!(rejection(&client.receive(&too_many)).contains("more than 20000 items"));
    let too_long = open(
        json!([chart(
            "c",
            line_scene(2, &"b".repeat(MAX_SCENE_STRING_CHARS + 1))
        )]),
        json!({}),
    );
    assert!(rejection(&client.receive(&too_long)).contains("longer than 16384"));
}

#[test]
fn other_props_and_data_keep_the_protocol_limits() {
    let big: Vec<u32> = (0..1001).collect();
    let cases = [
        open(
            json!([{ "id": "c", "type": "chart:Chart", "props": { "title": "Readings", "kind": "line", "rows": big, "scene": line_scene(3, "x") } }]),
            json!({}),
        ),
        open(
            json!([{ "id": "c", "type": "Text", "props": { "text": "hi", "scene": { "marks": big } } }]),
            json!({}),
        ),
        open(
            json!([chart("c", line_scene(3, "x"))]),
            json!({ "scene": { "marks": big } }),
        ),
        open(
            json!([{ "id": "c", "type": "Text", "props": { "text": "c".repeat(4097) } }]),
            json!({}),
        ),
        open(
            json!([{ "id": "c", "type": "chart:Chart", "props": { "title": "t".repeat(4097), "kind": "line", "rows": [] } }]),
            json!({}),
        ),
    ];
    for message in cases {
        let detail = rejection(&client().receive(&message));
        assert!(
            detail.contains("more than 1000 items") || detail.contains("longer than 4096"),
            "{detail}"
        );
    }
}
