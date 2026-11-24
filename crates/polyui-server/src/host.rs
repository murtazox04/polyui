use axum::http::HeaderMap;
use futures_util::future::BoxFuture;
use polyui_core::protocol::ActionSource;
use polyui_core::{ActionRegistry, CatalogSet, Locale, RefResolver};
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub id: String,
    pub name: Option<String>,
}

impl User {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: None,
        }
    }

    pub fn guest() -> Self {
        Self::new("guest")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmText {
    pub title: String,
    pub body: Option<String>,
    pub confirm_label: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct ActionCall<'a> {
    pub action: &'a str,
    pub args: &'a Map<String, Value>,
    pub values: &'a Map<String, Value>,
    pub detail: Option<&'a Map<String, Value>>,
    pub source: ActionSource,
    pub locale: Locale,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionOutcome {
    pub ok: bool,
    pub message: Option<String>,
    pub note: Option<String>,
}

impl ActionOutcome {
    pub fn ok() -> Self {
        Self {
            ok: true,
            message: None,
            note: None,
        }
    }

    pub fn rejected(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            message: Some(message.into()),
            note: None,
        }
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }
}

pub trait Host: Send + Sync + 'static {
    fn system_prompt(&self, user: &User, locale: Locale) -> String;

    fn actions(&self) -> &dyn ActionRegistry;

    fn refs<'a>(&'a self, user: &'a User) -> Box<dyn RefResolver + 'a>;

    fn confirm_text(
        &self,
        user: &User,
        action: &str,
        args: &Map<String, Value>,
        locale: Locale,
    ) -> ConfirmText;

    fn run_action<'a>(
        &'a self,
        user: &'a User,
        call: ActionCall<'a>,
    ) -> BoxFuture<'a, ActionOutcome>;

    fn catalogs(&self) -> CatalogSet {
        CatalogSet::builtin()
    }

    fn authenticate(&self, headers: &HeaderMap) -> Result<User, String> {
        let _ = headers;
        Ok(User::guest())
    }

    fn tools(&self) -> Vec<Value> {
        Vec::new()
    }

    fn call_tool<'a>(
        &'a self,
        user: &'a User,
        name: &'a str,
        input: &'a Value,
    ) -> BoxFuture<'a, Result<String, String>> {
        let _ = (user, input);
        Box::pin(async move { Err(format!("unknown tool \"{name}\"")) })
    }

    fn fallback_text(&self, locale: Locale) -> String {
        default_fallback(locale).to_string()
    }

    fn allow_http(&self) -> bool {
        false
    }
}

pub fn default_fallback(locale: Locale) -> &'static str {
    match locale {
        Locale::En => {
            "Sorry, I could not show that on screen. Please try asking again in a different way."
        }
        Locale::UzLatn => {
            "Kechirasiz, buni ekranda ko'rsata olmadim. Iltimos, boshqacha so'rab ko'ring."
        }
        Locale::UzCyrl => "Кечирасиз, буни экранда кўрсата олмадим. Илтимос, бошқача сўраб кўринг.",
        Locale::Ru => {
            "Извините, не получилось показать это на экране. Попробуйте спросить по-другому."
        }
    }
}
