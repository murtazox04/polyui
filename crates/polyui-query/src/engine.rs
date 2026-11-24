use crate::error::{QueryError, SetupError};
use crate::guard;
use crate::result::{self, ColumnType, QueryResult};
use crate::table::{ColumnInfo, Source, Table, TableInfo, is_plain_name};
use datafusion::arrow::datatypes::Schema;
use datafusion::datasource::MemTable;
use datafusion::error::DataFusionError;
use datafusion::execution::disk_manager::{DiskManagerBuilder, DiskManagerMode};
use datafusion::execution::runtime_env::RuntimeEnvBuilder;
use datafusion::logical_expr::TableSource;
use datafusion::prelude::{
    CsvReadOptions, DataFrame, ParquetReadOptions, SessionConfig, SessionContext,
};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

const BLOCKED_FUNCTIONS: [&str; 3] = ["repeat", "lpad", "rpad"];
const BLOCKED_TABLE_FUNCTIONS: [&str; 2] = ["generate_series", "range"];
const HIDDEN: &str = "<data>";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_rows: usize,
    pub timeout: Duration,
    pub memory_bytes: usize,
    pub max_sql_chars: usize,
    pub partitions: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_rows: 1000,
            timeout: Duration::from_secs(5),
            memory_bytes: 256 * 1024 * 1024,
            max_sql_chars: 8000,
            partitions: 2,
        }
    }
}

pub struct Engine {
    context: SessionContext,
    tables: Vec<TableInfo>,
    names: Vec<String>,
    allowed: Vec<Arc<dyn TableSource>>,
    hidden: Vec<String>,
    limits: Limits,
    executions: AtomicU64,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("tables", &self.names)
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

fn source_error(table: &str, error: impl std::fmt::Display) -> SetupError {
    SetupError::Source {
        table: table.to_string(),
        detail: error.to_string(),
    }
}

fn path_text(path: &Path) -> Result<String, String> {
    path.to_str()
        .map(str::to_string)
        .ok_or_else(|| format!("{} is not valid UTF-8", path.display()))
}

fn tails(path: &Path, shortest: usize) -> Vec<String> {
    let parts: Vec<String> = path
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .filter(|part| part != "/")
        .collect();
    (0..parts.len())
        .filter(|start| parts.len() - start >= shortest)
        .map(|start| parts[start..].join("/"))
        .collect()
}

fn hidden_forms(path: &Path) -> Vec<String> {
    let mut forms = vec![path.to_string_lossy().into_owned()];
    let full = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    forms.push(full.to_string_lossy().into_owned());
    forms.extend(tails(&full, 1));
    if let Some(parent) = full.parent() {
        forms.extend(tails(parent, 2));
    }
    forms.retain(|form| form.len() > 2);
    forms
}

fn missing_table(message: &str) -> Option<&str> {
    message
        .strip_prefix("table '")
        .and_then(|rest| rest.strip_suffix("' not found"))
}

impl Engine {
    pub async fn open(tables: Vec<Table>, limits: Limits) -> Result<Self, SetupError> {
        let runtime = RuntimeEnvBuilder::new()
            .with_memory_limit(limits.memory_bytes, 1.0)
            .with_disk_manager_builder(
                DiskManagerBuilder::default().with_mode(DiskManagerMode::Disabled),
            )
            .build_arc()
            .map_err(|error| source_error("runtime", error))?;
        let config = SessionConfig::new()
            .with_information_schema(false)
            .with_target_partitions(limits.partitions.max(1));
        let context = SessionContext::new_with_config_rt(config, runtime);
        for name in BLOCKED_FUNCTIONS {
            context.deregister_udf(name);
        }
        for name in BLOCKED_TABLE_FUNCTIONS {
            context.deregister_udtf(name);
        }
        let mut infos = Vec::new();
        let mut names = Vec::new();
        let mut allowed = Vec::new();
        let mut hidden = Vec::new();
        for table in tables {
            if !is_plain_name(&table.name) {
                return Err(SetupError::BadName(table.name));
            }
            if names.contains(&table.name) {
                return Err(SetupError::Duplicate(table.name));
            }
            if table.columns.is_empty() {
                return Err(SetupError::NoColumns(table.name));
            }
            let mut seen: Vec<&str> = Vec::new();
            for column in &table.columns {
                if !is_plain_name(&column.name) {
                    return Err(SetupError::BadName(column.name.clone()));
                }
                if seen.contains(&column.name.as_str()) {
                    return Err(SetupError::Duplicate(column.name.clone()));
                }
                seen.push(&column.name);
            }
            let frame = Self::read(&context, &table, &mut hidden).await?;
            for column in &table.columns {
                if frame
                    .schema()
                    .field_with_unqualified_name(&column.name)
                    .is_err()
                {
                    return Err(SetupError::MissingColumn {
                        table: table.name.clone(),
                        column: column.name.clone(),
                    });
                }
            }
            let frame = frame
                .select_columns(&seen)
                .map_err(|error| source_error(&table.name, error))?;
            allowed.extend(guard::scanned_sources(frame.logical_plan()));
            let rows = frame
                .clone()
                .count()
                .await
                .map_err(|error| source_error(&table.name, error))?;
            let columns = table
                .columns
                .iter()
                .map(|column| ColumnInfo {
                    name: column.name.clone(),
                    kind: frame
                        .schema()
                        .field_with_unqualified_name(&column.name)
                        .map_or(ColumnType::Other, |field| ColumnType::of(field.data_type())),
                    description: column.description.clone(),
                })
                .collect();
            context
                .register_table(table.name.as_str(), frame.into_view())
                .map_err(|error| source_error(&table.name, error))?;
            infos.push(TableInfo {
                name: table.name.clone(),
                description: table.description.clone(),
                rows,
                columns,
            });
            names.push(table.name);
        }
        hidden.sort_by_key(|form: &String| std::cmp::Reverse(form.len()));
        hidden.dedup();
        Ok(Self {
            context,
            tables: infos,
            names,
            allowed,
            hidden,
            limits,
            executions: AtomicU64::new(0),
        })
    }

    async fn read(
        context: &SessionContext,
        table: &Table,
        hidden: &mut Vec<String>,
    ) -> Result<DataFrame, SetupError> {
        match &table.source {
            Source::Csv(path) => {
                hidden.extend(hidden_forms(path));
                let text = path_text(path).map_err(|error| source_error(&table.name, error))?;
                context
                    .read_csv(text, CsvReadOptions::new().has_header(true))
                    .await
                    .map_err(|error| source_error(&table.name, error))
            }
            Source::Parquet(path) => {
                hidden.extend(hidden_forms(path));
                let text = path_text(path).map_err(|error| source_error(&table.name, error))?;
                context
                    .read_parquet(text, ParquetReadOptions::default())
                    .await
                    .map_err(|error| source_error(&table.name, error))
            }
            Source::Memory(batches) => {
                let schema = batches
                    .first()
                    .map(|batch| batch.schema())
                    .ok_or_else(|| source_error(&table.name, "no record batches"))?;
                let provider = MemTable::try_new(schema, vec![batches.clone()])
                    .map_err(|error| source_error(&table.name, error))?;
                context
                    .read_table(Arc::new(provider))
                    .map_err(|error| source_error(&table.name, error))
            }
        }
    }

    pub fn tables(&self) -> &[TableInfo] {
        &self.tables
    }

    pub fn limits(&self) -> Limits {
        self.limits
    }

    pub fn executions(&self) -> u64 {
        self.executions.load(Ordering::Relaxed)
    }

    pub fn describe(&self) -> String {
        let mut lines = vec![
            "Tables you can read with run_query. Only these tables and columns exist; SQL is read-only (DataFusion dialect, close to PostgreSQL).".to_string(),
        ];
        lines.extend(self.tables.iter().map(TableInfo::describe));
        lines.join("\n")
    }

    pub async fn run(&self, sql: &str) -> Result<QueryResult, QueryError> {
        let statement = guard::single_select(sql, self.limits.max_sql_chars)?;
        let state = self.context.state();
        let plan = state
            .statement_to_plan(statement)
            .await
            .map_err(|error| self.planning_error(&error))?;
        guard::check_plan(&plan, &self.allowed)?;
        self.executions.fetch_add(1, Ordering::Relaxed);
        let fetch = self.limits.max_rows.saturating_add(1);
        let work = async {
            let frame = self
                .context
                .execute_logical_plan(plan)
                .await?
                .limit(0, Some(fetch))?;
            let schema: Schema = frame.schema().as_arrow().clone();
            let batches = frame.collect().await?;
            Ok::<_, DataFusionError>((schema, batches))
        };
        let (schema, batches) = tokio::time::timeout(self.limits.timeout, work)
            .await
            .map_err(|_| QueryError::Timeout(self.limits.timeout))?
            .map_err(|error| self.running_error(&error))?;
        result::build(&schema, &batches, self.limits.max_rows).map_err(|_| QueryError::Failed)
    }

    fn redact(&self, text: &str) -> String {
        let mut clean = text.replace("file://", "");
        for form in &self.hidden {
            clean = clean.replace(form.as_str(), HIDDEN);
        }
        guard::first_line(&clean)
    }

    fn planning_error(&self, error: &DataFusionError) -> QueryError {
        match error.find_root() {
            DataFusionError::ResourcesExhausted(_) => QueryError::Memory,
            root @ (DataFusionError::Plan(_)
            | DataFusionError::SchemaError(..)
            | DataFusionError::SQL(..)
            | DataFusionError::NotImplemented(_)) => {
                let message = root.message();
                match missing_table(&message) {
                    Some(name) if name.contains(['/', '\\']) || name != self.redact(name) => {
                        QueryError::UnknownTable(None)
                    }
                    Some(name) => QueryError::UnknownTable(Some(name.to_string())),
                    None => QueryError::Invalid(self.redact(&message)),
                }
            }
            _ => QueryError::Failed,
        }
    }

    fn running_error(&self, error: &DataFusionError) -> QueryError {
        match error.find_root() {
            DataFusionError::ResourcesExhausted(_) => QueryError::Memory,
            root @ (DataFusionError::Plan(_) | DataFusionError::SchemaError(..)) => {
                QueryError::Invalid(self.redact(&root.message()))
            }
            _ => QueryError::Failed,
        }
    }
}
