#![forbid(unsafe_code)]

mod engine;
mod error;
mod guard;
mod result;
mod table;

pub use datafusion::arrow;
pub use engine::{Engine, Limits};
pub use error::{QueryError, SetupError};
pub use result::{Column, ColumnType, QueryResult};
pub use table::{ColumnInfo, ColumnSpec, Source, Table, TableInfo};
