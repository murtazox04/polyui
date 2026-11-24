use std::fmt;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryError {
    Empty,
    TooLong { max_chars: usize },
    Syntax(String),
    NotSingle { count: usize },
    NotSelect,
    ReadOnly,
    UnknownTable(Option<String>),
    Invalid(String),
    Timeout(Duration),
    Memory,
    Failed,
}

impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "the query is empty. Write one SELECT statement."),
            Self::TooLong { max_chars } => write!(
                f,
                "the query is longer than {max_chars} characters. Write a shorter SELECT."
            ),
            Self::Syntax(detail) => write!(f, "the SQL could not be parsed: {detail}"),
            Self::NotSingle { count } => write!(
                f,
                "send exactly one SELECT statement, not {count}. Remove the extra statements and semicolons."
            ),
            Self::NotSelect => write!(
                f,
                "only a single read-only SELECT (optionally with WITH) is allowed. Data cannot be changed, created, copied or explained."
            ),
            Self::ReadOnly => write!(
                f,
                "the query tries to change data or settings. Only read-only SELECT is allowed."
            ),
            Self::UnknownTable(Some(name)) => write!(
                f,
                "\"{name}\" is not a table you can read. Call describe_data to see the tables."
            ),
            Self::UnknownTable(None) => write!(
                f,
                "files cannot be read by path. Only the tables from describe_data exist."
            ),
            Self::Invalid(detail) => write!(f, "the query is not valid: {detail}"),
            Self::Timeout(limit) => write!(
                f,
                "the query took longer than {} ms and was stopped. Filter or aggregate more.",
                limit.as_millis()
            ),
            Self::Memory => write!(
                f,
                "the query needed more memory than allowed and was stopped. Aggregate more or return fewer rows."
            ),
            Self::Failed => write!(f, "the query failed while it was running."),
        }
    }
}

impl std::error::Error for QueryError {}

#[derive(Debug)]
pub enum SetupError {
    BadName(String),
    MissingColumn { table: String, column: String },
    NoColumns(String),
    Duplicate(String),
    Source { table: String, detail: String },
}

impl fmt::Display for SetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadName(name) => write!(
                f,
                "\"{name}\" is not a plain lowercase name (a-z, 0-9 and _, at most 64 characters)"
            ),
            Self::MissingColumn { table, column } => {
                write!(f, "table \"{table}\" has no column \"{column}\"")
            }
            Self::NoColumns(table) => write!(f, "table \"{table}\" lists no columns"),
            Self::Duplicate(name) => write!(f, "\"{name}\" is listed twice"),
            Self::Source { table, detail } => {
                write!(f, "table \"{table}\" could not be opened: {detail}")
            }
        }
    }
}

impl std::error::Error for SetupError {}
