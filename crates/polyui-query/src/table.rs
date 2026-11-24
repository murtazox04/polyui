use crate::result::ColumnType;
use datafusion::arrow::record_batch::RecordBatch;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum Source {
    Csv(PathBuf),
    Parquet(PathBuf),
    Memory(Vec<RecordBatch>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnSpec {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct Table {
    pub name: String,
    pub description: String,
    pub source: Source,
    pub columns: Vec<ColumnSpec>,
}

impl Table {
    pub fn new(name: impl Into<String>, source: Source) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            source,
            columns: Vec::new(),
        }
    }

    pub fn csv(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self::new(name, Source::Csv(path.into()))
    }

    pub fn parquet(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self::new(name, Source::Parquet(path.into()))
    }

    pub fn memory(name: impl Into<String>, batches: Vec<RecordBatch>) -> Self {
        Self::new(name, Source::Memory(batches))
    }

    pub fn about(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn column(mut self, name: impl Into<String>, description: impl Into<String>) -> Self {
        self.columns.push(ColumnSpec {
            name: name.into(),
            description: description.into(),
        });
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ColumnInfo {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: ColumnType,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TableInfo {
    pub name: String,
    pub description: String,
    pub rows: usize,
    pub columns: Vec<ColumnInfo>,
}

impl TableInfo {
    pub fn describe(&self) -> String {
        let mut lines = vec![if self.description.is_empty() {
            format!("{} ({} rows)", self.name, self.rows)
        } else {
            format!("{} ({} rows): {}", self.name, self.rows, self.description)
        }];
        lines.extend(self.columns.iter().map(|column| {
            if column.description.is_empty() {
                format!("  - {} {}", column.name, column.kind.as_str())
            } else {
                format!(
                    "  - {} {}: {}",
                    column.name,
                    column.kind.as_str(),
                    column.description
                )
            }
        }));
        lines.join("\n")
    }
}

pub(crate) fn is_plain_name(name: &str) -> bool {
    let mut chars = name.chars();
    name.len() <= 64
        && chars
            .next()
            .is_some_and(|first| first.is_ascii_lowercase() || first == '_')
        && chars.all(|rest| rest.is_ascii_lowercase() || rest.is_ascii_digit() || rest == '_')
}
