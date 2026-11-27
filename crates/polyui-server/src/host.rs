use axum::http::HeaderMap;
use futures_util::future::BoxFuture;
use polyui_core::protocol::ActionSource;
use polyui_core::{ActionRegistry, CatalogSet, Locale, RefResolver};

pub use polyui_core::embed::UiResource;
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

pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const REFUSED: i64 = -32000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMethod {
    CallTool,
    ReadResource,
}

impl AppMethod {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "tools/call" => Some(Self::CallTool),
            "resources/read" => Some(Self::ReadResource),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::CallTool => "tools/call",
            Self::ReadResource => "resources/read",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AppRequest<'a> {
    pub app: &'a str,
    pub method: AppMethod,
    pub params: &'a Map<String, Value>,
}

impl AppRequest<'_> {
    pub fn tool_name(&self) -> Option<&str> {
        match self.method {
            AppMethod::CallTool => self.params.get("name").and_then(Value::as_str),
            AppMethod::ReadResource => None,
        }
    }

    pub fn resource_uri(&self) -> Option<&str> {
        match self.method {
            AppMethod::ReadResource => self.params.get("uri").and_then(Value::as_str),
            AppMethod::CallTool => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppError {
    pub code: i64,
    pub message: String,
}

impl AppError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn not_available() -> Self {
        Self::new(METHOD_NOT_FOUND, "Method not found")
    }

    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(INVALID_PARAMS, message)
    }

    pub fn refused(message: impl Into<String>) -> Self {
        Self::new(REFUSED, message)
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

    fn ui_resource<'a>(
        &'a self,
        user: &'a User,
        uri: &'a str,
    ) -> BoxFuture<'a, Option<UiResource>> {
        let _ = (user, uri);
        Box::pin(async { None })
    }

    fn app_calls(&self) -> bool {
        false
    }

    fn app_confirm(
        &self,
        user: &User,
        request: AppRequest<'_>,
        locale: Locale,
    ) -> Option<ConfirmText> {
        let _ = (user, request, locale);
        None
    }

    fn app_request<'a>(
        &'a self,
        user: &'a User,
        request: AppRequest<'a>,
    ) -> BoxFuture<'a, Result<Value, AppError>> {
        let _ = (user, request);
        Box::pin(async { Err(AppError::not_available()) })
    }

    fn fallback_text(&self, locale: Locale) -> String {
        default_fallback(locale).to_string()
    }

    fn allow_http(&self) -> bool {
        false
    }

    #[cfg(feature = "query")]
    fn data(&self) -> Option<&polyui_query::Engine> {
        None
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
