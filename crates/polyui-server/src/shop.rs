use crate::host::{ActionCall, ActionOutcome, ConfirmText, Host, User};
use futures_util::future::BoxFuture;
use polyui_core::{ActionRegistry, ActionTable, Locale, RefResolver, StaticRefs};
use polyui_query::{Engine, Limits, SetupError, Table};
use serde_json::{Map, Value};
use std::path::Path;

pub const DEFAULT_DATA_DIR: &str = "examples/data/shop";
pub const TODAY: &str = "2026-11-24";

pub fn tables(dir: &Path) -> Vec<Table> {
    vec![
        Table::parquet("orders", dir.join("orders.parquet"))
            .about("One row per order, from 2026-01-01 to 2026-11-23.")
            .column("order_id", "Order number.")
            .column("order_date", "Day the order was placed.")
            .column("customer_id", "Buyer, joins customers.customer_id.")
            .column("product_id", "Product, joins products.product_id.")
            .column("quantity", "Units bought.")
            .column("unit_price", "Price of one unit in so'm at order time.")
            .column("discount", "Share taken off the order, 0 to 0.15.")
            .column("channel", "Where the order came from: web, app or store.")
            .column(
                "status",
                "paid or refunded. Revenue counts paid orders only.",
            ),
        Table::csv("products", dir.join("products.csv"))
            .about("The catalog.")
            .column("product_id", "Product id.")
            .column("name", "Product name.")
            .column("category", "Coffee, Tea, Equipment or Accessories.")
            .column("price", "List price in so'm."),
        Table::csv("customers", dir.join("customers.csv"))
            .about("Registered customers. Contact details are not available.")
            .column("customer_id", "Customer id.")
            .column("name", "Full name.")
            .column("city", "City in Uzbekistan.")
            .column("signup_date", "Day the customer registered."),
    ]
}

pub struct ShopHost {
    engine: Engine,
    actions: ActionTable,
}

impl ShopHost {
    pub async fn open(dir: impl AsRef<Path>) -> Result<Self, SetupError> {
        Self::open_with(dir, Limits::default()).await
    }

    pub async fn open_with(dir: impl AsRef<Path>, limits: Limits) -> Result<Self, SetupError> {
        let engine = Engine::open(tables(dir.as_ref()), limits).await?;
        Ok(Self {
            engine,
            actions: ActionTable::new(),
        })
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }
}

impl Host for ShopHost {
    fn system_prompt(&self, user: &User, locale: Locale) -> String {
        format!(
            "You are the analytics assistant of Choy va Qahva, a small online store in Uzbekistan that sells coffee, tea, brewing equipment and accessories. Today is {TODAY}. The signed in user is {}, the store owner.

Answer in the language the user writes in (Uzbek in Latin or Cyrillic script, Russian or English). If you cannot tell, use the app locale {}. Write titles and labels in that language. Money is in so'm.

When the owner asks how the store is doing, or asks for a report or a dashboard, work in this order:
1. Call describe_data once to learn the tables and columns.
2. Plan a dashboard of 3 or 4 panels that answers the question: usually one or two Stat panels with headline numbers and two chart:Chart panels (a trend over time and a breakdown).
3. Call run_query once per panel. Revenue is sum(quantity * unit_price * (1 - discount)) over orders with status = 'paid'. For a monthly trend use to_char(order_date, '%Y-%m') AS x. Chart queries must name their columns x and y (and optionally series) and stay small: aggregate, and keep a breakdown to 12 rows or fewer with ORDER BY and LIMIT. Stat queries return one row with one column; round money with round(..., 0).
4. Call show_ui once with root a chart:Dashboard (title, columns 2) whose children are the Stat and chart:Chart components. Chart rows are {{\"$ref\": \"query:<id>\"}}; Stat values are {{\"$ref\": \"stat:<id>\"}} with unit so'm where it is money. Use kind line or area for trends, bar for comparisons, pie only for shares of a whole with at most 6 slices.
5. Finish with one or two short sentences. Do not repeat the numbers in text; the dashboard shows them.

When the owner asks to change one panel, run a new query only for that panel if its data changes, and call update_ui on the open surface with only that one component. Never rebuild the whole dashboard for a one-panel change.",
            user.name.as_deref().unwrap_or(user.id.as_str()),
            locale.tag()
        )
    }

    fn actions(&self) -> &dyn ActionRegistry {
        &self.actions
    }

    fn refs<'a>(&'a self, user: &'a User) -> Box<dyn RefResolver + 'a> {
        let _ = user;
        Box::new(StaticRefs::new())
    }

    fn confirm_text(
        &self,
        user: &User,
        action: &str,
        args: &Map<String, Value>,
        locale: Locale,
    ) -> ConfirmText {
        let _ = (user, args, locale);
        ConfirmText {
            title: action.to_string(),
            body: None,
            confirm_label: None,
        }
    }

    fn run_action<'a>(
        &'a self,
        user: &'a User,
        call: ActionCall<'a>,
    ) -> BoxFuture<'a, ActionOutcome> {
        let _ = (user, call);
        Box::pin(async { ActionOutcome::rejected("This store has no actions.") })
    }

    fn data(&self) -> Option<&Engine> {
        Some(&self.engine)
    }
}
