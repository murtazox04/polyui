use crate::error::QueryError;
use datafusion::common::tree_node::TreeNodeRecursion;
use datafusion::execution::context::SQLOptions;
use datafusion::logical_expr::{LogicalPlan, TableSource};
use datafusion::sql::parser::{DFParserBuilder, Statement};
use datafusion::sql::sqlparser::ast::Statement as SqlStatement;
use datafusion::sql::sqlparser::dialect::GenericDialect;
use std::sync::Arc;

const RECURSION_LIMIT: usize = 64;

pub(crate) fn single_select(sql: &str, max_chars: usize) -> Result<Statement, QueryError> {
    if sql.trim().is_empty() {
        return Err(QueryError::Empty);
    }
    if sql.chars().count() > max_chars {
        return Err(QueryError::TooLong { max_chars });
    }
    let dialect = GenericDialect {};
    let mut statements = DFParserBuilder::new(sql)
        .with_dialect(&dialect)
        .with_recursion_limit(RECURSION_LIMIT)
        .build()
        .and_then(|mut parser| parser.parse_statements())
        .map_err(|error| QueryError::Syntax(first_line(&error.strip_backtrace())))?;
    if statements.len() != 1 {
        return Err(QueryError::NotSingle {
            count: statements.len(),
        });
    }
    match statements.pop_front() {
        Some(Statement::Statement(statement))
            if matches!(statement.as_ref(), SqlStatement::Query(_)) =>
        {
            Ok(Statement::Statement(statement))
        }
        _ => Err(QueryError::NotSelect),
    }
}

fn read_only_options() -> SQLOptions {
    SQLOptions::new()
        .with_allow_ddl(false)
        .with_allow_dml(false)
        .with_allow_copy(false)
        .with_allow_statements(false)
}

pub(crate) fn scanned_sources(plan: &LogicalPlan) -> Vec<Arc<dyn TableSource>> {
    let mut sources = Vec::new();
    let walked = plan.apply_with_subqueries(|node| {
        if let LogicalPlan::TableScan(scan) = node {
            sources.push(Arc::clone(&scan.source));
        }
        Ok(TreeNodeRecursion::Continue)
    });
    if walked.is_err() {
        sources.clear();
    }
    sources
}

pub(crate) fn check_plan(
    plan: &LogicalPlan,
    allowed: &[Arc<dyn TableSource>],
) -> Result<(), QueryError> {
    read_only_options()
        .verify_plan(plan)
        .map_err(|_| QueryError::ReadOnly)?;
    let mut problem = None;
    let walked = plan.apply_with_subqueries(|node| {
        let verdict = match node {
            LogicalPlan::TableScan(scan) => {
                let known = allowed
                    .iter()
                    .any(|source| Arc::ptr_eq(source, &scan.source));
                (!known).then(|| QueryError::UnknownTable(Some(scan.table_name.to_string())))
            }
            LogicalPlan::Dml(_)
            | LogicalPlan::Ddl(_)
            | LogicalPlan::Copy(_)
            | LogicalPlan::Statement(_)
            | LogicalPlan::Explain(_)
            | LogicalPlan::Analyze(_)
            | LogicalPlan::Extension(_) => Some(QueryError::NotSelect),
            _ => None,
        };
        match verdict {
            Some(found) => {
                problem = Some(found);
                Ok(TreeNodeRecursion::Stop)
            }
            None => Ok(TreeNodeRecursion::Continue),
        }
    });
    match (walked, problem) {
        (_, Some(found)) => Err(found),
        (Err(_), None) => Err(QueryError::Failed),
        (Ok(_), None) => Ok(()),
    }
}

pub(crate) fn first_line(text: &str) -> String {
    text.lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(300)
        .collect()
}
