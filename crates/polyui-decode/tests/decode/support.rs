use polyui_core::{ActionSpec, ActionTable, Catalog, CatalogSet};
use polyui_decode::{Layout, ToolGrammar};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Case {
    pub file: String,
    pub value: Value,
}

impl Case {
    pub fn tool(&self) -> &str {
        self.value["tool"].as_str().unwrap_or("show_ui")
    }

    pub fn ok(&self) -> bool {
        self.value["expect"]["ok"] == true
    }

    pub fn raw_text(&self) -> Option<&str> {
        self.value.get("input_text").and_then(Value::as_str)
    }

    pub fn catalogs(&self) -> CatalogSet {
        let mut catalogs = CatalogSet::builtin();
        if let Some(extension) = self.value.get("extension") {
            let catalog = Catalog::from_value(extension).expect("extension catalog parses");
            catalogs = catalogs
                .with(catalog)
                .expect("extension catalog is accepted");
        }
        if let Some(declared) = self.value.get("catalogs") {
            let declared: Vec<String> =
                serde_json::from_value(declared.clone()).expect("declared catalogs are strings");
            catalogs = catalogs.restrict(&declared);
        }
        catalogs
    }

    pub fn actions(&self) -> ActionTable {
        let mut actions = ActionTable::new();
        for (name, spec) in self.value["actions"]
            .as_object()
            .cloned()
            .unwrap_or_default()
        {
            actions.register(
                &name,
                ActionSpec::from_value(&spec).expect("action spec parses"),
            );
        }
        actions
    }

    pub fn grammar(&self) -> ToolGrammar {
        let grammar = ToolGrammar::new(&self.catalogs(), &self.actions()).expect("grammar builds");
        if self.raw_text().is_some() {
            grammar.with_layout(Layout::Flexible)
        } else {
            grammar
        }
    }

    pub fn call_text(&self, grammar: &ToolGrammar) -> String {
        match self.raw_text() {
            Some(raw) => format!(
                "<tool_call>\n{{\"name\": \"{}\", \"arguments\": {raw}}}\n</tool_call>",
                self.tool()
            ),
            None => grammar.render_call(self.tool(), &self.value["input"]),
        }
    }
}

pub fn fixture_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/conformance/validate")
}

pub fn cases() -> Vec<Case> {
    let mut files: Vec<PathBuf> = fs::read_dir(fixture_directory())
        .expect("fixture directory exists")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|path| Case {
            file: path
                .file_name()
                .expect("fixture has a file name")
                .to_string_lossy()
                .into_owned(),
            value: serde_json::from_str(&fs::read_to_string(&path).expect("fixture is readable"))
                .expect("fixture is JSON"),
        })
        .collect()
}

pub fn case(file: &str) -> Case {
    cases()
        .into_iter()
        .find(|case| case.file == file)
        .expect("fixture exists")
}
