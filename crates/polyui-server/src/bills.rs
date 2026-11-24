use crate::host::{ActionCall, ActionOutcome, ConfirmText, Host, UiResource, User};
use axum::http::HeaderMap;
use futures_util::future::BoxFuture;
use polyui_core::client::MCP_APP_MIME_TYPE;
use polyui_core::{
    ActionRegistry, ActionSpec, ActionTable, CatalogSet, Locale, PropKind, RefResolver,
};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::sync::Mutex;

pub const HOUSEHOLD_USER: &str = "karimov";
pub const TODAY: &str = "2026-11-24";
pub const MONTHS: [&str; 6] = [
    "2026-06", "2026-07", "2026-08", "2026-09", "2026-10", "2026-11",
];
pub const CARD_MASK: &str = "**** 4471";
pub const TARIFF_APP_URI: &str = "ui://bills/tariff";
pub const TARIFF_APP_HTML: &str = include_str!("tariff-app.html");
pub const TARIFF_MESSAGE: &str = "tariff_message";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Service {
    pub key: &'static str,
    pub name: &'static str,
    pub provider: &'static str,
    pub account_number: &'static str,
    pub bill: &'static str,
    pub due_tiyin: u64,
    pub due_date: &'static str,
    pub unit: &'static str,
    pub usage: [f64; 6],
    pub charges_tiyin: [u64; 6],
}

pub const SERVICES: [Service; 4] = [
    Service {
        key: "gas",
        name: "Gas",
        provider: "Hududgaz Toshkent",
        account_number: "4017 2944 7100 0083",
        bill: "b_gas_2611",
        due_tiyin: 2_932_219,
        due_date: "2026-12-10",
        unit: "m3",
        usage: [38.0, 35.0, 36.0, 44.0, 71.0, 112.0],
        charges_tiyin: [995_600, 917_000, 943_200, 1_152_800, 1_860_200, 2_932_219],
    },
    Service {
        key: "electricity",
        name: "Electricity",
        provider: "Toshkent shahar elektr tarmoqlari",
        account_number: "1290 5531 0047 2218",
        bill: "b_power_2611",
        due_tiyin: 8_451_000,
        due_date: "2026-12-05",
        unit: "kWh",
        usage: [310.0, 355.0, 342.0, 290.0, 275.0, 318.0],
        charges_tiyin: [
            8_238_000, 9_435_000, 9_090_000, 7_708_000, 7_309_000, 8_451_000,
        ],
    },
    Service {
        key: "water",
        name: "Water",
        provider: "Suvsoz",
        account_number: "7700 1834 5521 0961",
        bill: "b_water_2611",
        due_tiyin: 1_890_000,
        due_date: "2026-12-15",
        unit: "m3",
        usage: [14.2, 15.1, 14.8, 13.6, 12.9, 13.3],
        charges_tiyin: [
            2_017_000, 2_145_000, 2_102_000, 1_932_000, 1_832_000, 1_890_000,
        ],
    },
    Service {
        key: "internet",
        name: "Internet",
        provider: "Uztelecom",
        account_number: "5502 0019 3384 7710",
        bill: "b_net_2611",
        due_tiyin: 9_900_000,
        due_date: "2026-12-01",
        unit: "GB",
        usage: [182.0, 205.0, 198.0, 221.0, 240.0, 236.0],
        charges_tiyin: [
            9_900_000, 9_900_000, 9_900_000, 9_900_000, 9_900_000, 9_900_000,
        ],
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Offer {
    pub id: &'static str,
    pub name: &'static str,
    pub speed_mbps: u32,
    pub price_tiyin: u64,
}

pub const OFFERS: [Offer; 3] = [
    Offer {
        id: "oila_50",
        name: "Oila 50",
        speed_mbps: 50,
        price_tiyin: 9_900_000,
    },
    Offer {
        id: "oila_100",
        name: "Oila 100",
        speed_mbps: 100,
        price_tiyin: 14_900_000,
    },
    Offer {
        id: "turbo_200",
        name: "Turbo 200",
        speed_mbps: 200,
        price_tiyin: 21_900_000,
    },
];

pub fn format_som(tiyin: u64) -> String {
    let whole = (tiyin / 100).to_string();
    let mut grouped = String::new();
    for (position, digit) in whole.chars().enumerate() {
        if position > 0 && (whole.len() - position) % 3 == 0 {
            grouped.push(' ');
        }
        grouped.push(digit);
    }
    format!("{grouped},{:02}", tiyin % 100)
}

pub fn mask(account_number: &str) -> String {
    let digits: String = account_number
        .chars()
        .filter(char::is_ascii_digit)
        .collect();
    let tail: String = digits
        .chars()
        .skip(digits.len().saturating_sub(4))
        .collect();
    format!("**** {tail}")
}

pub fn service(key: &str) -> Option<&'static Service> {
    SERVICES.iter().find(|service| service.key == key)
}

fn service_for_bill(bill: &str) -> Option<&'static Service> {
    SERVICES.iter().find(|service| service.bill == bill)
}

pub fn usage_rows(service: &Service) -> Value {
    let rows: Vec<Value> = MONTHS
        .iter()
        .zip(service.usage.iter().zip(service.charges_tiyin.iter()))
        .map(|(month, (amount, charge))| {
            json!({ "month": month, "usage": amount, "unit": service.unit, "charge_som": (*charge as f64) / 100.0 })
        })
        .collect();
    json!({ "service": service.key, "provider": service.provider, "rows": rows })
}

fn enum_arg(values: &[&str], description: &str) -> Value {
    json!({ "kind": "enum", "values": values, "required": true, "description": description })
}

fn action_table(tariff_app: bool) -> ActionTable {
    let bills: Vec<&str> = SERVICES.iter().map(|service| service.bill).collect();
    let services: Vec<&str> = SERVICES.iter().map(|service| service.key).collect();
    let offers: Vec<&str> = OFFERS.iter().map(|offer| offer.id).collect();
    let spec =
        |value: Value| ActionSpec::from_value(&value).unwrap_or_else(|_| ActionSpec::new(true));
    let table = ActionTable::new()
        .with(
            "pay_bill",
            spec(json!({
                "description": "Pay one bill in full from the family card.",
                "requires_confirm": true,
                "args": { "bill": enum_arg(&bills, "Bill id.") }
            })),
        )
        .with(
            "pick_offer",
            spec(json!({
                "description": "Choose an internet tariff. Use with on_change of a ChoiceGroup and send its path in with.",
                "requires_confirm": false,
                "args": { "offer": { "kind": "enum", "values": offers, "description": "Offer id, when the button is for one offer." } }
            })),
        )
        .with(
            "show_usage",
            spec(json!({
                "description": "The user wants the usage history of one service.",
                "requires_confirm": false,
                "args": { "service": enum_arg(&services, "Service key.") }
            })),
        );
    if !tariff_app {
        return table;
    }
    table.with(
        TARIFF_MESSAGE,
        spec(json!({
            "description": "The tariff calculator app sent a message for you. Use as on_message of the embed:Embed that shows it. The text arrives as the action detail.",
            "requires_confirm": false,
            "args": {}
        })),
    )
}

pub fn tariff_app() -> UiResource {
    let offers: Vec<Value> = OFFERS
        .iter()
        .map(|offer| {
            json!({ "id": offer.id, "name": offer.name, "speed_mbps": offer.speed_mbps, "price_tiyin": offer.price_tiyin })
        })
        .collect();
    let internet = service("internet").map_or(0.0, |service| service.usage[5]);
    let content = json!({
        "uri": TARIFF_APP_URI,
        "mimeType": MCP_APP_MIME_TYPE,
        "text": TARIFF_APP_HTML,
        "_meta": { "ui": { "csp": {} } }
    });
    let summary = format!(
        "The family used {internet} GB of internet this month on {}.",
        OFFERS[0].name
    );
    let input = Map::from_iter([
        ("usage_gb".to_string(), json!(internet)),
        ("current".to_string(), json!(OFFERS[0].id)),
    ]);
    let result = Map::from_iter([
        (
            "content".to_string(),
            json!([{ "type": "text", "text": summary }]),
        ),
        ("structuredContent".to_string(), json!({ "offers": offers })),
    ]);
    UiResource::new(content)
        .with_tool_input(input)
        .with_tool_result(result)
}

pub struct BillsHost {
    actions: ActionTable,
    tariff_app: bool,
    paid: Mutex<BTreeSet<String>>,
    picked: Mutex<Option<String>>,
}

impl Default for BillsHost {
    fn default() -> Self {
        Self::new()
    }
}

impl BillsHost {
    pub fn new() -> Self {
        Self {
            actions: action_table(false),
            tariff_app: false,
            paid: Mutex::new(BTreeSet::new()),
            picked: Mutex::new(None),
        }
    }

    pub fn with_tariff_app(mut self) -> Self {
        self.actions = action_table(true);
        self.tariff_app = true;
        self
    }

    pub fn is_paid(&self, bill: &str) -> bool {
        self.paid
            .lock()
            .map(|paid| paid.contains(bill))
            .unwrap_or(false)
    }

    pub fn picked_offer(&self) -> Option<String> {
        self.picked.lock().ok().and_then(|picked| picked.clone())
    }

    fn pay(&self, args: &Map<String, Value>, locale: Locale) -> ActionOutcome {
        let Some(service) = args
            .get("bill")
            .and_then(Value::as_str)
            .and_then(service_for_bill)
        else {
            return ActionOutcome::rejected(text(locale, Phrase::UnknownBill));
        };
        let Ok(mut paid) = self.paid.lock() else {
            return ActionOutcome::rejected(text(locale, Phrase::TryLater));
        };
        if !paid.insert(service.bill.to_string()) {
            return ActionOutcome::rejected(text(locale, Phrase::AlreadyPaid))
                .with_note(format!("Bill {} was already paid.", service.bill));
        }
        ActionOutcome::ok()
            .with_message(format!(
                "{} {} so'm.",
                text(locale, Phrase::Paid),
                format_som(service.due_tiyin)
            ))
            .with_note(format!(
                "Bill {} ({}) is paid. The {} balance is now 0 so'm.",
                service.bill, service.provider, service.key
            ))
    }

    fn pick(&self, call: &ActionCall<'_>) -> ActionOutcome {
        let chosen = call
            .args
            .get("offer")
            .and_then(Value::as_str)
            .or_else(|| call.values.values().find_map(Value::as_str));
        let Some(offer) = chosen.and_then(|id| OFFERS.iter().find(|offer| offer.id == id)) else {
            return ActionOutcome::rejected(text(call.locale, Phrase::UnknownOffer));
        };
        if let Ok(mut picked) = self.picked.lock() {
            *picked = Some(offer.id.to_string());
        }
        ActionOutcome::ok()
            .with_message(format!(
                "{}: {}",
                text(call.locale, Phrase::Picked),
                offer.name
            ))
            .with_note(format!(
                "Tariff {} ({} Mbit/s, {} so'm a month) is chosen and starts on 2026-12-01.",
                offer.name,
                offer.speed_mbps,
                format_som(offer.price_tiyin)
            ))
    }
}

#[derive(Clone, Copy)]
enum Phrase {
    UnknownBill,
    AlreadyPaid,
    TryLater,
    Paid,
    UnknownOffer,
    Picked,
    PayTitle,
    PayLabel,
    FromCard,
}

fn text(locale: Locale, phrase: Phrase) -> &'static str {
    match (locale, phrase) {
        (Locale::UzLatn, Phrase::UnknownBill) => "Bunday hisob topilmadi.",
        (Locale::UzLatn, Phrase::AlreadyPaid) => "Bu hisob allaqachon to'langan.",
        (Locale::UzLatn, Phrase::TryLater) => "Keyinroq qayta urinib ko'ring.",
        (Locale::UzLatn, Phrase::Paid) => "To'landi:",
        (Locale::UzLatn, Phrase::UnknownOffer) => "Bunday tarif topilmadi.",
        (Locale::UzLatn, Phrase::Picked) => "Tanlangan tarif",
        (Locale::UzLatn, Phrase::PayTitle) => "To'lash",
        (Locale::UzLatn, Phrase::PayLabel) => "To'lash",
        (Locale::UzLatn, Phrase::FromCard) => "karta",
        (Locale::UzCyrl, Phrase::UnknownBill) => "Бундай ҳисоб топилмади.",
        (Locale::UzCyrl, Phrase::AlreadyPaid) => "Бу ҳисоб аллақачон тўланган.",
        (Locale::UzCyrl, Phrase::TryLater) => "Кейинроқ қайта уриниб кўринг.",
        (Locale::UzCyrl, Phrase::Paid) => "Тўланди:",
        (Locale::UzCyrl, Phrase::UnknownOffer) => "Бундай тариф топилмади.",
        (Locale::UzCyrl, Phrase::Picked) => "Танланган тариф",
        (Locale::UzCyrl, Phrase::PayTitle) => "Тўлаш",
        (Locale::UzCyrl, Phrase::PayLabel) => "Тўлаш",
        (Locale::UzCyrl, Phrase::FromCard) => "карта",
        (Locale::Ru, Phrase::UnknownBill) => "Такой счёт не найден.",
        (Locale::Ru, Phrase::AlreadyPaid) => "Этот счёт уже оплачен.",
        (Locale::Ru, Phrase::TryLater) => "Попробуйте позже.",
        (Locale::Ru, Phrase::Paid) => "Оплачено:",
        (Locale::Ru, Phrase::UnknownOffer) => "Такой тариф не найден.",
        (Locale::Ru, Phrase::Picked) => "Выбран тариф",
        (Locale::Ru, Phrase::PayTitle) => "Оплатить",
        (Locale::Ru, Phrase::PayLabel) => "Оплатить",
        (Locale::Ru, Phrase::FromCard) => "карта",
        (Locale::En, Phrase::UnknownBill) => "This bill was not found.",
        (Locale::En, Phrase::AlreadyPaid) => "This bill is already paid.",
        (Locale::En, Phrase::TryLater) => "Please try again later.",
        (Locale::En, Phrase::Paid) => "Paid",
        (Locale::En, Phrase::UnknownOffer) => "This tariff was not found.",
        (Locale::En, Phrase::Picked) => "Chosen tariff",
        (Locale::En, Phrase::PayTitle) => "Pay",
        (Locale::En, Phrase::PayLabel) => "Pay",
        (Locale::En, Phrase::FromCard) => "card",
    }
}

pub struct HouseholdRefs<'a> {
    user: &'a User,
}

pub const MASK_CHARS: u64 = 16;

impl RefResolver for HouseholdRefs<'_> {
    fn source_kind(&self, source: &str) -> Option<PropKind> {
        matches!(source, "account" | "card").then_some(PropKind::String {
            min: Some(1),
            max: Some(MASK_CHARS),
        })
    }

    fn resolve(&self, source: &str, key: &str) -> Result<Value, String> {
        if self.user.id != HOUSEHOLD_USER {
            return Err("this user has no access to the household".into());
        }
        match source {
            "account" => service(key)
                .map(|service| Value::from(mask(service.account_number)))
                .ok_or_else(|| {
                    format!(
                        "no account \"{key}\"; known: {}",
                        SERVICES.map(|service| service.key).join(", ")
                    )
                }),
            "card" if key == "main" => Ok(Value::from(CARD_MASK)),
            "card" => Err(format!("no card \"{key}\"; known: main")),
            _ => Err(format!("unknown source \"{source}\"")),
        }
    }
}

fn bills_table(host: &BillsHost) -> String {
    SERVICES
        .iter()
        .map(|service| {
            let status = if host.is_paid(service.bill) {
                "paid".to_string()
            } else {
                format!("{} so'm due by {}", format_som(service.due_tiyin), service.due_date)
            };
            format!(
                "- {} ({}): bill id {}, {status}, account number ref {{\"$ref\": \"account:{}\"}}, this month {} {}",
                service.key,
                service.provider,
                service.bill,
                service.key,
                service.usage[5],
                service.unit
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn offers_table() -> String {
    OFFERS
        .iter()
        .map(|offer| {
            format!(
                "- {} \"{}\": {} Mbit/s, {} so'm a month",
                offer.id,
                offer.name,
                offer.speed_mbps,
                format_som(offer.price_tiyin)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn tariff_app_guide() -> String {
    format!(
        "\n\nIf this client draws embed@1, there is also an interactive internet tariff calculator, an MCP App at {TARIFF_APP_URI}. When the user wants to work out which internet tariff suits them, show it with {{\"type\": \"embed:Embed\", \"props\": {{\"resource\": \"{TARIFF_APP_URI}\", \"title\": \"<what it shows, in the user's language>\", \"height\": \"large\", \"on_message\": {{\"action\": \"{TARIFF_MESSAGE}\"}}}}}}, usually as a child of a Card. A {TARIFF_MESSAGE} event carries text the user wrote through the app; treat it like a message the user typed. No other ui:// resources exist."
    )
}

impl Host for BillsHost {
    fn system_prompt(&self, user: &User, locale: Locale) -> String {
        let guide = if self.tariff_app {
            tariff_app_guide()
        } else {
            String::new()
        };
        format!(
            "You are the assistant inside a household bills app for the Karimov family in Tashkent, Uzbekistan. Today is {TODAY}. The signed in user is {}.

Always answer in the language the user writes in: Uzbek in Latin script, Uzbek in Cyrillic, Russian or English. If you cannot tell, use the app locale {}. Write labels and titles in that language too. Money is in so'm, written like 29 322,19 so'm.

Prefer UI to long text. Show amounts with Stat inside a Card. Give every unpaid bill a Button with tone primary and on_press {{\"action\": \"pay_bill\", \"args\": {{\"bill\": \"<bill id>\"}}}}. The app asks the user to confirm every payment itself. To compare internet tariffs use a ChoiceGroup with value \"/offer\", one option per tariff (id = offer id) and on_change {{\"action\": \"pick_offer\", \"with\": [\"/offer\"]}}. For usage history call get_usage and draw a chart:Chart with kind bar or line, x = month, y = usage or charge, using exactly the rows it returns. Never invent numbers.

Account numbers are secret. Never write them. Where the user should see one, write {{\"$ref\": \"account:<service>\"}} as the prop value, for example as the hint of a Stat. The family card is {{\"$ref\": \"card:main\"}}. Each of these refs is text of up to 16 characters.

Current bills:
{}

Internet tariffs:
{}{guide}",
            user.name.as_deref().unwrap_or("a member of the family"),
            locale.tag(),
            bills_table(self),
            offers_table()
        )
    }

    fn catalogs(&self) -> CatalogSet {
        if self.tariff_app {
            CatalogSet::builtin().with_embed()
        } else {
            CatalogSet::builtin()
        }
    }

    fn ui_resource<'a>(
        &'a self,
        user: &'a User,
        uri: &'a str,
    ) -> BoxFuture<'a, Option<UiResource>> {
        Box::pin(async move {
            (self.tariff_app && user.id == HOUSEHOLD_USER && uri == TARIFF_APP_URI).then(tariff_app)
        })
    }

    fn actions(&self) -> &dyn ActionRegistry {
        &self.actions
    }

    fn refs<'a>(&'a self, user: &'a User) -> Box<dyn RefResolver + 'a> {
        Box::new(HouseholdRefs { user })
    }

    fn confirm_text(
        &self,
        user: &User,
        action: &str,
        args: &Map<String, Value>,
        locale: Locale,
    ) -> ConfirmText {
        let _ = user;
        let service = args
            .get("bill")
            .and_then(Value::as_str)
            .and_then(service_for_bill);
        match (action, service) {
            ("pay_bill", Some(service)) => ConfirmText {
                title: format!("{}: {}", text(locale, Phrase::PayTitle), service.provider),
                body: Some(format!(
                    "{} so'm, {} {CARD_MASK}",
                    format_som(service.due_tiyin),
                    text(locale, Phrase::FromCard)
                )),
                confirm_label: Some(text(locale, Phrase::PayLabel).to_string()),
            },
            _ => ConfirmText {
                title: action.to_string(),
                body: None,
                confirm_label: None,
            },
        }
    }

    fn run_action<'a>(
        &'a self,
        user: &'a User,
        call: ActionCall<'a>,
    ) -> BoxFuture<'a, ActionOutcome> {
        Box::pin(async move {
            if user.id != HOUSEHOLD_USER {
                return ActionOutcome::rejected(text(call.locale, Phrase::TryLater));
            }
            match call.action {
                "pay_bill" => self.pay(call.args, call.locale),
                "pick_offer" => self.pick(&call),
                "show_usage" => match call
                    .args
                    .get("service")
                    .and_then(Value::as_str)
                    .and_then(service)
                {
                    Some(service) => ActionOutcome::ok()
                        .with_note(format!("Usage data: {}", usage_rows(service))),
                    None => ActionOutcome::rejected(text(call.locale, Phrase::UnknownBill)),
                },
                TARIFF_MESSAGE if self.tariff_app => ActionOutcome::ok().with_note(
                    "The detail text was written by the user inside the tariff calculator.",
                ),
                _ => ActionOutcome::rejected(text(call.locale, Phrase::TryLater)),
            }
        })
    }

    fn authenticate(&self, headers: &HeaderMap) -> Result<User, String> {
        let _ = headers;
        Ok(User {
            id: HOUSEHOLD_USER.into(),
            name: Some("Dilnoza Karimova".into()),
        })
    }

    fn tools(&self) -> Vec<Value> {
        let services: Vec<&str> = SERVICES.iter().map(|service| service.key).collect();
        vec![json!({
            "name": "get_usage",
            "description": "Returns monthly usage and charges for one service over the last six months. Call it before drawing a usage chart.",
            "strict": true,
            "input_schema": {
                "type": "object",
                "properties": { "service": { "type": "string", "enum": services } },
                "required": ["service"],
                "additionalProperties": false
            }
        })]
    }

    fn call_tool<'a>(
        &'a self,
        user: &'a User,
        name: &'a str,
        input: &'a Value,
    ) -> BoxFuture<'a, Result<String, String>> {
        Box::pin(async move {
            if user.id != HOUSEHOLD_USER {
                return Err("this user has no access to the household".into());
            }
            if name != "get_usage" {
                return Err(format!("unknown tool \"{name}\""));
            }
            let key = input
                .get("service")
                .and_then(Value::as_str)
                .unwrap_or_default();
            service(key)
                .map(|service| usage_rows(service).to_string())
                .ok_or_else(|| {
                    format!(
                        "unknown service \"{key}\"; known: {}",
                        SERVICES.map(|service| service.key).join(", ")
                    )
                })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_and_masks() {
        assert_eq!(format_som(2_932_219), "29 322,19");
        assert_eq!(format_som(5), "0,05");
        assert_eq!(format_som(100_000_000), "1 000 000,00");
        assert_eq!(mask("4017 2944 7100 0083"), "**** 0083");
    }

    #[test]
    fn refs_need_the_household_user() {
        let owner = User::new(HOUSEHOLD_USER);
        let stranger = User::new("someone");
        let host = BillsHost::new();
        assert_eq!(
            host.refs(&owner).resolve("account", "gas"),
            Ok(json!("**** 0083"))
        );
        assert!(host.refs(&stranger).resolve("account", "gas").is_err());
        assert!(host.refs(&owner).resolve("account", "phone").is_err());
        assert!(!host.refs(&owner).has_source("bill"));
    }

    #[test]
    fn prompt_never_contains_full_account_numbers() {
        let host = BillsHost::new();
        let prompt = host.system_prompt(&User::new(HOUSEHOLD_USER), Locale::UzLatn);
        for service in SERVICES {
            assert!(!prompt.contains(service.account_number));
            assert!(!prompt.contains(&mask(service.account_number)));
            assert!(prompt.contains(service.bill));
        }
        assert!(prompt.contains("29 322,19"));
        assert!(!prompt.contains(TARIFF_APP_URI));
    }

    #[tokio::test]
    async fn the_tariff_app_is_offered_only_when_turned_on() {
        let owner = User::new(HOUSEHOLD_USER);
        let plain = BillsHost::new();
        assert!(plain.catalogs().get("embed:Embed").is_none());
        assert!(plain.actions().action(TARIFF_MESSAGE).is_none());
        assert!(plain.ui_resource(&owner, TARIFF_APP_URI).await.is_none());
        let host = BillsHost::new().with_tariff_app();
        assert!(host.catalogs().get("embed:Embed").is_some());
        assert!(host.actions().action(TARIFF_MESSAGE).is_some());
        assert!(
            host.system_prompt(&owner, Locale::En)
                .contains(TARIFF_APP_URI)
        );
        let resource = host.ui_resource(&owner, TARIFF_APP_URI).await.unwrap();
        let app = resource.app(TARIFF_APP_URI).unwrap();
        assert_eq!(app["resource"]["mimeType"], MCP_APP_MIME_TYPE);
        assert_eq!(
            app["tool_result"]["structuredContent"]["offers"][2]["id"],
            "turbo_200"
        );
        assert!(host.ui_resource(&owner, "ui://bills/other").await.is_none());
        assert!(
            host.ui_resource(&User::new("someone"), TARIFF_APP_URI)
                .await
                .is_none()
        );
    }
}
