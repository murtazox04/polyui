use futures_util::future::join_all;
use polyui_chart::format_number;
use polyui_core::validate::{Location, Problem, split_ref};
use polyui_core::{CatalogSet, PropKind, RefResolver};
use polyui_query::{Engine, QueryResult};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

pub const DESCRIBE_TOOL: &str = "describe_data";
pub const QUERY_TOOL: &str = "run_query";
pub const ROWS_SOURCE: &str = "query";
pub const VALUE_SOURCE: &str = "stat";
pub const PREVIEW_ROWS: usize = 10;
pub const MAX_SAVED: usize = 64;

const CHART_TYPE: &str = "chart:Chart";
const STAT_TYPE: &str = "Stat";

pub const GUIDE: &str = "How to answer with data from the host:
- Call describe_data first to see the tables and columns. Never guess table or column names.
- Call run_query with one read-only SELECT. Its result is saved under an id like q1 and you get a short preview. The data stays on the server.
- Numbers reach the screen only through refs. A chart:Chart takes {\"rows\": {\"$ref\": \"query:q1\"}} when the query names its columns x and y (and optionally series). A Stat takes {\"value\": {\"$ref\": \"stat:q2\"}} when the query returns one row with one column. The server runs the saved query again when it draws, and you only see [ref].
- Never copy numbers from a preview into props, titles or text.
- To change one panel, run a new query if the data changes and call update_ui with only that component. The other panels are not recomputed.";

pub fn tools() -> Vec<Value> {
    vec![
        json!({
            "name": DESCRIBE_TOOL,
            "description": "Lists the tables and columns you can query, with their types and row counts. Call it before writing SQL.",
            "strict": true,
            "input_schema": { "type": "object", "properties": {}, "required": [], "additionalProperties": false }
        }),
        json!({
            "name": QUERY_TOOL,
            "description": "Runs one read-only SELECT on the host's data and saves it under an id like q1. Returns the columns, the row count and a short preview. Put the saved result on screen with {\"$ref\": \"query:<id>\"} for chart rows or {\"$ref\": \"stat:<id>\"} for a Stat value.",
            "strict": true,
            "input_schema": {
                "type": "object",
                "properties": { "sql": { "type": "string", "description": "One SELECT statement. For chart rows name the columns x, y and optionally series." } },
                "required": ["sql"],
                "additionalProperties": false
            }
        }),
    ]
}

#[derive(Debug, Default)]
pub struct SavedQueries {
    entries: BTreeMap<String, String>,
    counter: u64,
}

impl SavedQueries {
    pub fn save(&mut self, sql: &str) -> Option<String> {
        if self.entries.len() >= MAX_SAVED {
            return None;
        }
        self.counter += 1;
        let id = format!("q{}", self.counter);
        self.entries.insert(id.clone(), sql.to_string());
        Some(id)
    }

    pub fn entries(&self) -> &BTreeMap<String, String> {
        &self.entries
    }

    pub fn sql(&self, id: &str) -> Option<&str> {
        self.entries.get(id).map(String::as_str)
    }

    pub fn ids(&self) -> String {
        if self.entries.is_empty() {
            "none".into()
        } else {
            self.entries.keys().cloned().collect::<Vec<_>>().join(", ")
        }
    }
}

fn rows_problem(id: &str, result: &QueryResult, max_rows: usize) -> Option<String> {
    let names: Vec<&str> = result
        .columns
        .iter()
        .map(|column| column.name.as_str())
        .collect();
    let allowed = ["x", "y", "series"];
    if result.column("x").is_none()
        || result.column("y").is_none()
        || names.iter().any(|name| !allowed.contains(name))
    {
        return Some(format!(
            "query {id} has columns {}; chart rows need columns named x and y, and optionally series. Run it again with AS x, AS y.",
            names.join(", ")
        ));
    }
    if result
        .column("y")
        .is_some_and(|column| !column.kind.is_numeric())
    {
        return Some(format!(
            "column y of query {id} is {}; it must be a number.",
            result
                .column("y")
                .map_or("other", |column| column.kind.as_str())
        ));
    }
    if result.truncated || result.rows.len() > max_rows {
        return Some(format!(
            "query {id} returns more than {max_rows} rows; a chart shows at most {max_rows}. Aggregate it or add LIMIT."
        ));
    }
    None
}

fn value_problem(id: &str, result: &QueryResult, max_chars: usize) -> Option<String> {
    match result.single_value() {
        None => Some(format!(
            "query {id} returns {} rows and {} columns; a Stat value needs exactly one row with one column.",
            result.rows.len(),
            result.columns.len()
        )),
        Some(Value::Null) => Some(format!(
            "query {id} returns null; a Stat value needs a number or text. Use COALESCE."
        )),
        Some(Value::String(text)) if text.chars().count() > max_chars => Some(format!(
            "query {id} returns text longer than {max_chars} characters, too long for a Stat value."
        )),
        Some(_) => None,
    }
}

fn chart_rows(result: &QueryResult) -> Value {
    let rows: Vec<Value> = result
        .rows
        .iter()
        .filter_map(|row| {
            let x = row.get("x").filter(|value| !value.is_null())?;
            let y = row.get("y").filter(|value| value.is_number())?;
            let mut out = Map::new();
            out.insert("x".into(), x.clone());
            out.insert("y".into(), y.clone());
            match row.get("series") {
                Some(Value::String(name)) => {
                    out.insert("series".into(), Value::String(name.clone()));
                }
                Some(Value::Null) | None => {}
                Some(other) => {
                    out.insert("series".into(), Value::String(other.to_string()));
                }
            }
            Some(Value::Object(out))
        })
        .collect();
    Value::Array(rows)
}

fn stat_value(result: &QueryResult, locale: polyui_chart::Locale) -> Value {
    match result.single_value() {
        Some(Value::Number(number)) => number.as_f64().map_or(Value::Null, |value| {
            Value::String(format_number(value, locale))
        }),
        Some(Value::String(text)) => Value::String(text.clone()),
        Some(Value::Bool(flag)) => Value::String(flag.to_string()),
        _ => Value::Null,
    }
}

pub fn run_summary(id: &str, result: &QueryResult, chart_max: usize, stat_max: usize) -> String {
    let columns = result
        .columns
        .iter()
        .map(|column| format!("{} ({})", column.name, column.kind.as_str()))
        .collect::<Vec<_>>()
        .join(", ");
    let count = if result.truncated {
        format!(
            "more than {} rows (only the first {} were kept)",
            result.rows.len(),
            result.rows.len()
        )
    } else {
        format!("{} rows", result.rows.len())
    };
    let mut lines = vec![format!("Saved as {id}: {count}. Columns: {columns}.")];
    match rows_problem(id, result, chart_max) {
        None => lines.push(format!(
            "Use it as chart rows: {{\"$ref\": \"{ROWS_SOURCE}:{id}\"}}."
        )),
        Some(problem) => lines.push(format!("Not usable as chart rows: {problem}")),
    }
    if value_problem(id, result, stat_max).is_none() {
        lines.push(format!(
            "Use it as a Stat value: {{\"$ref\": \"{VALUE_SOURCE}:{id}\"}}."
        ));
    }
    let shown = result.rows.len().min(PREVIEW_ROWS);
    lines.push(format!(
        "Preview, {shown} of {} rows, for planning only (do not copy these numbers into props):",
        result.rows.len()
    ));
    lines.push(result.preview(PREVIEW_ROWS));
    lines.join("\n")
}

#[derive(Debug, Clone, PartialEq)]
pub struct RefUse {
    pub list: String,
    pub index: usize,
    pub component: String,
    pub prop: String,
    pub source: String,
    pub key: String,
}

fn collect_refs(value: &Value, path: &str, found: &mut Vec<(String, String, String)>) {
    match value {
        Value::Object(map) => {
            if map.len() == 1
                && let Some((source, key)) =
                    map.get("$ref").and_then(Value::as_str).and_then(split_ref)
                && (source == ROWS_SOURCE || source == VALUE_SOURCE)
            {
                found.push((path.to_string(), source.to_string(), key.to_string()));
                return;
            }
            for (name, item) in map {
                collect_refs(item, &format!("{path}.{name}"), found);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                collect_refs(item, &format!("{path}[{index}]"), found);
            }
        }
        _ => {}
    }
}

pub fn query_refs(input: &Value, list: &str) -> Vec<RefUse> {
    let mut uses = Vec::new();
    let Some(components) = input.get(list).and_then(Value::as_array) else {
        return uses;
    };
    for (index, component) in components.iter().enumerate() {
        let id = component
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let Some(props) = component.get("props").and_then(Value::as_object) else {
            continue;
        };
        for (name, value) in props {
            let mut found = Vec::new();
            collect_refs(value, name, &mut found);
            uses.extend(found.into_iter().map(|(prop, source, key)| RefUse {
                list: list.to_string(),
                index,
                component: id.to_string(),
                prop,
                source,
                key,
            }));
        }
    }
    uses
}

#[derive(Debug, Clone, Default)]
pub struct QueryRefs {
    rows_kind: Option<PropKind>,
    value_kind: Option<PropKind>,
    values: BTreeMap<String, Value>,
}

fn kind_of(catalogs: &CatalogSet, type_name: &str, prop: &str) -> Option<PropKind> {
    catalogs
        .get(type_name)
        .and_then(|definition| definition.prop(prop))
        .map(|spec| spec.kind.clone())
}

impl QueryRefs {
    pub fn new(catalogs: &CatalogSet) -> Self {
        Self {
            rows_kind: kind_of(catalogs, CHART_TYPE, "rows"),
            value_kind: kind_of(catalogs, STAT_TYPE, "value"),
            values: BTreeMap::new(),
        }
    }

    fn chart_max(&self) -> usize {
        match &self.rows_kind {
            Some(PropKind::Array { max: Some(max), .. }) => {
                usize::try_from(*max).unwrap_or(usize::MAX)
            }
            _ => usize::MAX,
        }
    }

    fn stat_max(&self) -> usize {
        match &self.value_kind {
            Some(PropKind::String { max: Some(max), .. }) => {
                usize::try_from(*max).unwrap_or(usize::MAX)
            }
            _ => usize::MAX,
        }
    }

    pub fn summary(&self, id: &str, result: &QueryResult) -> String {
        run_summary(id, result, self.chart_max(), self.stat_max())
    }

    pub async fn fill(
        mut self,
        engine: &Engine,
        saved: &BTreeMap<String, String>,
        uses: &[RefUse],
        locale: polyui_chart::Locale,
    ) -> Result<Self, Vec<Problem>> {
        let mut problems = Vec::new();
        let mut wanted: Vec<&str> = Vec::new();
        for usage in uses {
            if saved.contains_key(&usage.key) {
                if !wanted.contains(&usage.key.as_str()) {
                    wanted.push(&usage.key);
                }
            } else {
                problems.push(problem(
                    usage,
                    format!(
                        "query {} does not exist. Saved queries: {}. Call run_query first.",
                        usage.key,
                        if saved.is_empty() {
                            "none".to_string()
                        } else {
                            saved.keys().cloned().collect::<Vec<_>>().join(", ")
                        }
                    ),
                ));
            }
        }
        let runs = join_all(wanted.iter().map(|id| async move {
            let sql = saved.get(*id).map(String::as_str).unwrap_or_default();
            (*id, engine.run(sql).await)
        }))
        .await;
        let results: BTreeMap<&str, _> = runs.into_iter().collect();
        for usage in uses {
            let Some(outcome) = results.get(usage.key.as_str()) else {
                continue;
            };
            let result = match outcome {
                Ok(result) => result,
                Err(error) => {
                    problems.push(problem(
                        usage,
                        format!("query {} failed when it ran again: {error}", usage.key),
                    ));
                    continue;
                }
            };
            let reference = format!("{}:{}", usage.source, usage.key);
            if usage.source == ROWS_SOURCE {
                match rows_problem(&usage.key, result, self.chart_max()) {
                    Some(message) => problems.push(problem(usage, message)),
                    None => {
                        self.values.insert(reference, chart_rows(result));
                    }
                }
            } else {
                match value_problem(&usage.key, result, self.stat_max()) {
                    Some(message) => problems.push(problem(usage, message)),
                    None => {
                        self.values.insert(reference, stat_value(result, locale));
                    }
                }
            }
        }
        if problems.is_empty() {
            Ok(self)
        } else {
            Err(problems)
        }
    }
}

fn problem(usage: &RefUse, message: String) -> Problem {
    Problem::new(
        Location::Item {
            list: usage.list.clone(),
            index: usage.index,
            id: (!usage.component.is_empty()).then(|| usage.component.clone()),
            prop: Some(usage.prop.clone()),
        },
        message,
    )
}

impl RefResolver for QueryRefs {
    fn source_kind(&self, source: &str) -> Option<PropKind> {
        match source {
            ROWS_SOURCE => self.rows_kind.clone(),
            VALUE_SOURCE => self.value_kind.clone(),
            _ => None,
        }
    }

    fn resolve(&self, source: &str, key: &str) -> Result<Value, String> {
        self.values
            .get(&format!("{source}:{key}"))
            .cloned()
            .ok_or_else(|| format!("no saved result for {source}:{key}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refs_are_found_with_their_component() {
        let input = json!({ "components": [
            { "id": "d", "type": "chart:Dashboard", "props": { "title": "Shop" } },
            { "id": "c", "type": "chart:Chart", "props": { "title": "Sales", "kind": "bar", "rows": { "$ref": "query:q1" } } },
            { "id": "s", "type": "Stat", "props": { "label": "Orders", "value": { "$ref": "stat:q2" }, "hint": { "$ref": "account:gas" } } }
        ] });
        let uses = query_refs(&input, "components");
        assert_eq!(uses.len(), 2);
        assert_eq!(uses[0].component, "c");
        assert_eq!(uses[0].prop, "rows");
        assert_eq!(
            (uses[1].source.as_str(), uses[1].key.as_str()),
            ("stat", "q2")
        );
    }

    #[test]
    fn declared_kinds_come_from_the_catalog() {
        let refs = QueryRefs::new(&CatalogSet::builtin());
        assert!(matches!(
            refs.source_kind("query"),
            Some(PropKind::Array { .. })
        ));
        assert!(matches!(
            refs.source_kind("stat"),
            Some(PropKind::String { .. })
        ));
        assert_eq!(refs.chart_max(), 1000);
        assert_eq!(refs.stat_max(), 40);
        assert!(refs.source_kind("account").is_none());
    }
}
