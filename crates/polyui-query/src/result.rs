use datafusion::arrow::array::{Array, ArrayRef, AsArray};
use datafusion::arrow::compute::cast;
use datafusion::arrow::datatypes::{DataType, Float64Type, Int64Type, Schema, UInt64Type};
use datafusion::arrow::error::ArrowError;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::arrow::util::display::{ArrayFormatter, FormatOptions};
use serde::Serialize;
use serde_json::{Map, Number, Value};

pub const MAX_CELL_CHARS: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ColumnType {
    Integer,
    Number,
    Text,
    Boolean,
    Date,
    Timestamp,
    Other,
}

impl ColumnType {
    pub fn of(data_type: &DataType) -> Self {
        match data_type {
            DataType::Int8
            | DataType::Int16
            | DataType::Int32
            | DataType::Int64
            | DataType::UInt8
            | DataType::UInt16
            | DataType::UInt32
            | DataType::UInt64 => Self::Integer,
            DataType::Float16
            | DataType::Float32
            | DataType::Float64
            | DataType::Decimal32(..)
            | DataType::Decimal64(..)
            | DataType::Decimal128(..)
            | DataType::Decimal256(..) => Self::Number,
            DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => Self::Text,
            DataType::Boolean => Self::Boolean,
            DataType::Date32 | DataType::Date64 => Self::Date,
            DataType::Timestamp(..) => Self::Timestamp,
            _ => Self::Other,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Integer => "integer",
            Self::Number => "number",
            Self::Text => "text",
            Self::Boolean => "boolean",
            Self::Date => "date",
            Self::Timestamp => "timestamp",
            Self::Other => "other",
        }
    }

    pub fn is_numeric(self) -> bool {
        matches!(self, Self::Integer | Self::Number)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Column {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: ColumnType,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QueryResult {
    pub columns: Vec<Column>,
    pub rows: Vec<Map<String, Value>>,
    pub truncated: bool,
}

impl QueryResult {
    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }

    pub fn column(&self, name: &str) -> Option<&Column> {
        self.columns.iter().find(|column| column.name == name)
    }

    pub fn single_value(&self) -> Option<&Value> {
        match (self.rows.as_slice(), self.columns.as_slice()) {
            ([row], [column]) => row.get(&column.name),
            _ => None,
        }
    }

    pub fn preview(&self, max_rows: usize) -> String {
        let header = self
            .columns
            .iter()
            .map(|column| column.name.as_str())
            .collect::<Vec<_>>()
            .join(" | ");
        let mut lines = vec![header];
        lines.extend(self.rows.iter().take(max_rows).map(|row| {
            self.columns
                .iter()
                .map(|column| cell_text(row.get(&column.name).unwrap_or(&Value::Null)))
                .collect::<Vec<_>>()
                .join(" | ")
        }));
        lines.join("\n")
    }
}

fn cell_text(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::String(text) => text.chars().take(80).collect(),
        other => other.to_string(),
    }
}

fn clip(text: String) -> String {
    if text.chars().count() <= MAX_CELL_CHARS {
        text
    } else {
        text.chars().take(MAX_CELL_CHARS).collect()
    }
}

fn cells(array: &ArrayRef) -> Result<Vec<Value>, ArrowError> {
    match ColumnType::of(array.data_type()) {
        ColumnType::Boolean => Ok(array
            .as_boolean()
            .iter()
            .map(|cell| cell.map_or(Value::Null, Value::Bool))
            .collect()),
        ColumnType::Integer if array.data_type() == &DataType::UInt64 => Ok(array
            .as_primitive::<UInt64Type>()
            .iter()
            .map(|cell| cell.map_or(Value::Null, Value::from))
            .collect()),
        ColumnType::Integer => {
            let wide = cast(array, &DataType::Int64)?;
            Ok(wide
                .as_primitive::<Int64Type>()
                .iter()
                .map(|cell| cell.map_or(Value::Null, Value::from))
                .collect())
        }
        ColumnType::Number => {
            let wide = cast(array, &DataType::Float64)?;
            Ok(wide
                .as_primitive::<Float64Type>()
                .iter()
                .map(|cell| {
                    cell.and_then(Number::from_f64)
                        .map_or(Value::Null, Value::Number)
                })
                .collect())
        }
        _ => {
            let formatter = ArrayFormatter::try_new(array.as_ref(), &FormatOptions::default())?;
            Ok((0..array.len())
                .map(|index| {
                    if array.is_null(index) {
                        Value::Null
                    } else {
                        Value::String(clip(formatter.value(index).to_string()))
                    }
                })
                .collect())
        }
    }
}

pub(crate) fn build(
    schema: &Schema,
    batches: &[RecordBatch],
    max_rows: usize,
) -> Result<QueryResult, ArrowError> {
    let columns: Vec<Column> = schema
        .fields()
        .iter()
        .map(|field| Column {
            name: field.name().clone(),
            kind: ColumnType::of(field.data_type()),
        })
        .collect();
    let mut rows = Vec::new();
    let mut truncated = false;
    for batch in batches {
        let values = batch
            .columns()
            .iter()
            .map(cells)
            .collect::<Result<Vec<_>, _>>()?;
        for index in 0..batch.num_rows() {
            if rows.len() == max_rows {
                truncated = true;
                break;
            }
            let row: Map<String, Value> = columns
                .iter()
                .zip(values.iter())
                .map(|(column, cells)| {
                    (
                        column.name.clone(),
                        cells.get(index).cloned().unwrap_or(Value::Null),
                    )
                })
                .collect();
            rows.push(row);
        }
    }
    Ok(QueryResult {
        columns,
        rows,
        truncated,
    })
}
