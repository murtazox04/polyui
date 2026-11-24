use polyui_query::arrow::array::{ArrayRef, Int64Array, StringArray};
use polyui_query::arrow::datatypes::{DataType, Field, Schema};
use polyui_query::arrow::record_batch::RecordBatch;
use polyui_query::{ColumnType, Engine, Limits, QueryError, SetupError, Table};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

const ORDERS: usize = 1169;

fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/data/shop")
}

fn shop_tables() -> Vec<Table> {
    let dir = data_dir();
    let notes = RecordBatch::try_new(
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("text", DataType::Utf8, false),
            Field::new("secret", DataType::Utf8, false),
        ])),
        vec![
            Arc::new(Int64Array::from(vec![1, 2])) as ArrayRef,
            Arc::new(StringArray::from(vec!["open", "closed"])) as ArrayRef,
            Arc::new(StringArray::from(vec!["s1", "s2"])) as ArrayRef,
        ],
    )
    .unwrap();
    vec![
        Table::parquet("orders", dir.join("orders.parquet"))
            .column("order_id", "")
            .column("order_date", "")
            .column("customer_id", "")
            .column("product_id", "")
            .column("quantity", "")
            .column("unit_price", "")
            .column("discount", "")
            .column("channel", "")
            .column("status", ""),
        Table::csv("products", dir.join("products.csv"))
            .column("product_id", "")
            .column("name", "")
            .column("category", "")
            .column("price", ""),
        Table::csv("customers", dir.join("customers.csv"))
            .column("customer_id", "")
            .column("name", "")
            .column("city", "")
            .column("signup_date", ""),
        Table::memory("notes", vec![notes])
            .column("id", "")
            .column("text", ""),
    ]
}

async fn engine_with(limits: Limits) -> Engine {
    Engine::open(shop_tables(), limits).await.unwrap()
}

async fn engine() -> Engine {
    engine_with(Limits::default()).await
}

fn file_bytes() -> Vec<Vec<u8>> {
    ["orders.parquet", "products.csv", "customers.csv"]
        .iter()
        .map(|name| std::fs::read(data_dir().join(name)).unwrap())
        .collect()
}

fn assert_safe(error: &QueryError) {
    let text = error.to_string();
    let dir = data_dir();
    let full = dir.canonicalize().unwrap();
    for form in [dir.to_string_lossy(), full.to_string_lossy()] {
        assert!(!text.contains(form.as_ref()), "path leaked: {text}");
    }
    assert!(!text.contains("orders.parquet"), "file name leaked: {text}");
    assert!(!text.contains("customers.csv"), "file name leaked: {text}");
}

async fn rejected(engine: &Engine, sql: &str) -> QueryError {
    let before = engine.executions();
    let error = engine
        .run(sql)
        .await
        .expect_err(&format!("should be rejected: {sql}"));
    assert_safe(&error);
    assert_eq!(engine.executions(), before, "{sql} reached execution");
    error
}

#[tokio::test]
async fn plain_select_works_with_types() {
    let engine = engine().await;
    let result = engine
        .run("SELECT category, count(*) AS n, sum(price) AS total FROM products GROUP BY category ORDER BY category")
        .await
        .unwrap();
    assert_eq!(result.rows.len(), 4);
    assert_eq!(result.column("category").unwrap().kind, ColumnType::Text);
    assert_eq!(result.column("n").unwrap().kind, ColumnType::Integer);
    assert_eq!(result.rows[0]["category"], "Accessories");
    assert!(!result.truncated);
    let dated = engine
        .run("SELECT order_date FROM orders ORDER BY order_id LIMIT 1")
        .await
        .unwrap();
    assert_eq!(dated.columns[0].kind, ColumnType::Date);
    assert_eq!(dated.rows[0]["order_date"], "2026-01-01");
    let count = engine
        .run("SELECT count(*) AS n FROM orders")
        .await
        .unwrap();
    assert_eq!(count.single_value(), Some(&serde_json::json!(ORDERS)));
    let notes = engine
        .run("SELECT text FROM notes ORDER BY id")
        .await
        .unwrap();
    assert_eq!(notes.rows[1]["text"], "closed");
    assert_eq!(engine.tables()[0].rows, ORDERS);
}

#[tokio::test]
async fn writes_and_ddl_are_rejected() {
    let engine = engine().await;
    let before = file_bytes();
    for sql in [
        "DROP TABLE orders",
        "INSERT INTO orders (order_id) VALUES (1)",
        "UPDATE orders SET quantity = 0",
        "DELETE FROM orders",
        "CREATE TABLE copy AS SELECT * FROM orders",
        "CREATE VIEW v AS SELECT * FROM orders",
        "CREATE EXTERNAL TABLE leak STORED AS CSV LOCATION '/etc/hosts'",
        "COPY orders TO '/tmp/polyui-copy-test.csv'",
        "COPY (SELECT * FROM orders) TO '/tmp/polyui-copy-test.csv' STORED AS CSV",
        "EXPLAIN SELECT * FROM orders",
        "SET datafusion.execution.batch_size = 1",
        "SHOW TABLES",
        "TRUNCATE orders",
    ] {
        let error = rejected(&engine, sql).await;
        assert!(
            matches!(error, QueryError::NotSelect | QueryError::Syntax(_)),
            "{sql}: {error:?}"
        );
    }
    for sql in [
        "SELECT * INTO stolen FROM orders",
        "WITH x AS (SELECT 1) INSERT INTO orders (order_id) SELECT 1",
    ] {
        rejected(&engine, sql).await;
    }
    assert!(!std::path::Path::new("/tmp/polyui-copy-test.csv").exists());
    rejected(&engine, "SELECT * FROM stolen").await;
    assert_eq!(file_bytes(), before);
    let count = engine
        .run("SELECT count(*) AS n FROM orders")
        .await
        .unwrap();
    assert_eq!(count.single_value(), Some(&serde_json::json!(ORDERS)));
}

#[tokio::test]
async fn only_one_statement_is_allowed() {
    let engine = engine().await;
    let error = rejected(&engine, "SELECT 1; DROP TABLE orders").await;
    assert_eq!(error, QueryError::NotSingle { count: 2 });
    let error = rejected(&engine, "SELECT 1; SELECT 2").await;
    assert_eq!(error, QueryError::NotSingle { count: 2 });
    assert_eq!(rejected(&engine, "   ").await, QueryError::Empty);
    let long = format!("SELECT 1 {}", " ".repeat(9000));
    assert!(matches!(
        rejected(&engine, &long).await,
        QueryError::TooLong { .. }
    ));
}

#[tokio::test]
async fn tables_and_columns_outside_the_whitelist_are_unknown() {
    let engine = engine().await;
    for sql in [
        "SELECT email FROM customers",
        "SELECT phone FROM customers",
        "SELECT supplier_cost FROM products",
        "SELECT secret FROM notes",
        "SELECT * FROM secrets",
        "SELECT * FROM information_schema.tables",
        "SELECT * FROM other.public.orders",
        "SELECT name FROM customers WHERE customer_id IN (SELECT customer_id FROM payments)",
    ] {
        let error = rejected(&engine, sql).await;
        assert!(
            matches!(error, QueryError::Invalid(_) | QueryError::UnknownTable(_)),
            "{sql}: {error:?}"
        );
    }
    let everything = engine.run("SELECT * FROM customers LIMIT 1").await.unwrap();
    let names: Vec<&str> = everything
        .columns
        .iter()
        .map(|column| column.name.as_str())
        .collect();
    assert_eq!(names, ["customer_id", "name", "city", "signup_date"]);
}

#[tokio::test]
async fn table_functions_and_file_paths_are_rejected() {
    let engine = engine().await;
    let real = data_dir().join("customers.csv");
    let real = real.canonicalize().unwrap();
    for sql in [
        "SELECT * FROM read_csv('/etc/passwd')".to_string(),
        "SELECT * FROM read_parquet('/tmp/x.parquet')".to_string(),
        "SELECT * FROM generate_series(1, 1000000000)".to_string(),
        "SELECT * FROM range(10)".to_string(),
        "SELECT * FROM '/etc/passwd'".to_string(),
        format!("SELECT * FROM '{}'", real.display()),
        "SELECT * FROM 'examples/data/shop/customers.csv'".to_string(),
        "SELECT repeat('x', 1000000000)".to_string(),
        "SELECT lpad('x', 1000000000)".to_string(),
    ] {
        rejected(&engine, &sql).await;
    }
}

#[tokio::test]
async fn errors_never_name_the_data_files() {
    let engine = engine().await;
    for sql in [
        "SELECT nosuch FROM orders",
        "SELECT order_date + 'x' FROM orders",
        "SELECT FROM WHERE",
        "SELECT CAST(name AS INT) FROM customers",
    ] {
        if let Err(error) = engine.run(sql).await {
            assert_safe(&error);
        }
    }
}

#[tokio::test]
async fn huge_results_are_truncated() {
    let engine = engine_with(Limits {
        max_rows: 100,
        ..Limits::default()
    })
    .await;
    let result = engine.run("SELECT * FROM orders").await.unwrap();
    assert_eq!(result.rows.len(), 100);
    assert!(result.truncated);
    let small = engine
        .run("SELECT * FROM orders ORDER BY order_id LIMIT 100")
        .await
        .unwrap();
    assert_eq!(small.rows.len(), 100);
    assert!(!small.truncated);
}

#[tokio::test]
async fn slow_queries_time_out() {
    let engine = engine_with(Limits {
        timeout: Duration::from_millis(300),
        ..Limits::default()
    })
    .await;
    let started = Instant::now();
    let error = engine
        .run("SELECT count(*) FROM orders a, orders b, orders c WHERE a.quantity + b.quantity + c.quantity = 7")
        .await
        .unwrap_err();
    assert_eq!(error, QueryError::Timeout(Duration::from_millis(300)));
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn memory_limit_stops_big_aggregations() {
    let engine = engine_with(Limits {
        memory_bytes: 2 * 1024 * 1024,
        timeout: Duration::from_secs(30),
        ..Limits::default()
    })
    .await;
    let error = engine
        .run("SELECT a.order_id * 100000 + b.order_id AS k, count(*) AS n FROM orders a, orders b GROUP BY k")
        .await
        .unwrap_err();
    assert_eq!(error, QueryError::Memory);
}

#[tokio::test]
async fn user_text_spliced_into_sql_cannot_write() {
    let engine = engine().await;
    let before = file_bytes();
    let payloads = [
        "Robert'); DROP TABLE orders; --",
        "x' ; DELETE FROM orders WHERE '1'='1",
        "'; COPY orders TO '/tmp/polyui-injected.csv'; --",
        "Tashkent' UNION SELECT name, city FROM customers --",
        "' OR 1=1 --",
        "'; CREATE EXTERNAL TABLE t STORED AS CSV LOCATION '/etc/hosts'; SELECT '",
    ];
    for payload in payloads {
        let sql = format!("SELECT count(*) AS n FROM customers WHERE city = '{payload}'");
        if let Err(error) = engine.run(&sql).await {
            assert_safe(&error);
        }
    }
    assert!(!std::path::Path::new("/tmp/polyui-injected.csv").exists());
    assert_eq!(file_bytes(), before);
    let count = engine
        .run("SELECT count(*) AS n FROM orders")
        .await
        .unwrap();
    assert_eq!(count.single_value(), Some(&serde_json::json!(ORDERS)));
}

#[tokio::test]
async fn setup_rejects_bad_whitelists() {
    let dir = data_dir();
    let missing = Engine::open(
        vec![Table::csv("products", dir.join("products.csv")).column("nope", "")],
        Limits::default(),
    )
    .await
    .unwrap_err();
    assert!(matches!(missing, SetupError::MissingColumn { .. }));
    let odd = Engine::open(
        vec![Table::csv("Products; DROP", dir.join("products.csv")).column("name", "")],
        Limits::default(),
    )
    .await
    .unwrap_err();
    assert!(matches!(odd, SetupError::BadName(_)));
    let empty = Engine::open(
        vec![Table::csv("products", dir.join("products.csv"))],
        Limits::default(),
    )
    .await
    .unwrap_err();
    assert!(matches!(empty, SetupError::NoColumns(_)));
}

#[tokio::test]
async fn describe_lists_only_whitelisted_columns() {
    let engine = engine().await;
    let text = engine.describe();
    assert!(text.contains("orders (1169 rows)"));
    assert!(text.contains("signup_date date"));
    assert!(!text.contains("email"));
    assert!(!text.contains("supplier_cost"));
    assert!(!text.contains(&data_dir().to_string_lossy().to_string()));
}
