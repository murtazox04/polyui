use crate::catalogs::Catalogs;
use crate::errors::{catalog_error, chart_error};
use crate::host::{self, chart_options, chart_spec};
use crate::surface::Surface;
use napi::{Env, Error, Result, Status};
use napi_derive::napi;
use polyui_chart::{ChartScene, ChartSpec, Palette, compile, describe, to_svg};
use polyui_core::protocol::Component;
use polyui_core::summary;
use polyui_core::tools;
use serde_json::Value;

#[napi(object)]
pub struct ChartOptions {
    pub locale: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub dark: Option<bool>,
}

#[napi(object)]
pub struct PatchParts {
    pub upsert: Option<Value>,
    pub remove: Option<Vec<String>>,
    pub root: Option<String>,
}

fn spec(env: &Env, props: Value) -> Result<ChartSpec> {
    chart_spec(props)
        .map_err(|error| chart_error(env, format!("chart props could not be read: {error}")))
}

fn scene(env: &Env, props: Value, options: &Option<ChartOptions>) -> Result<ChartScene> {
    let (locale, width, height) = match options {
        Some(options) => (options.locale.as_deref(), options.width, options.height),
        None => (None, None, None),
    };
    compile(
        &spec(env, props)?,
        &chart_options(locale.unwrap_or("en"), width, height),
    )
    .map_err(|error| chart_error(env, error.to_string()))
}

#[napi]
pub fn tool_definitions(
    env: Env,
    catalogs: &Catalogs,
    actions: Option<Value>,
    strict: Option<bool>,
    profile: Option<String>,
) -> Result<Value> {
    let table = host::action_table(&actions.unwrap_or_default())
        .map_err(|failure| catalog_error(&env, &failure))?;
    let options = host::tool_options(strict.unwrap_or(false), profile.as_deref())
        .map_err(|message| Error::new(Status::InvalidArg, message))?;
    Ok(tools::tool_definitions(&catalogs.inner, &table, options))
}

#[napi]
pub fn open_message(
    surface_id: String,
    surface: &Surface,
    seq: Option<u32>,
    fallback: Option<String>,
) -> Value {
    host::open_message(
        &surface_id,
        &surface.inner,
        u64::from(seq.unwrap_or(1)),
        fallback,
    )
}

#[napi]
pub fn patch_message(surface_id: String, seq: u32, parts: Option<PatchParts>) -> Result<Value> {
    let parts = parts.unwrap_or(PatchParts {
        upsert: None,
        remove: None,
        root: None,
    });
    let upsert = match parts.upsert {
        Some(upsert) => serde_json::from_value::<Vec<Component>>(upsert).map_err(|error| {
            Error::new(
                Status::InvalidArg,
                format!("upsert is not a list of components: {error}"),
            )
        })?,
        None => Vec::new(),
    };
    Ok(host::patch_message(
        &surface_id,
        u64::from(seq),
        upsert,
        parts.remove.unwrap_or_default(),
        parts.root,
    ))
}

#[napi]
pub fn close_message(surface_id: String, seq: u32) -> Value {
    host::close_message(&surface_id, u64::from(seq))
}

#[napi]
pub fn closed_summary(surface_id: String) -> String {
    summary::closed(&surface_id)
}

#[napi]
pub fn compile_chart(env: Env, props: Value, options: Option<ChartOptions>) -> Result<Value> {
    let scene = scene(&env, props, &options)?;
    serde_json::to_value(&scene).map_err(|error| chart_error(&env, error.to_string()))
}

#[napi]
pub fn chart_svg(env: Env, props: Value, options: Option<ChartOptions>) -> Result<String> {
    let dark = options
        .as_ref()
        .and_then(|options| options.dark)
        .unwrap_or(false);
    let scene = scene(&env, props, &options)?;
    let palette = if dark {
        Palette::dark()
    } else {
        Palette::light()
    };
    Ok(to_svg(&scene, &palette))
}

#[napi]
pub fn describe_chart(env: Env, props: Value, locale: Option<String>) -> Result<String> {
    Ok(describe(
        &spec(&env, props)?,
        polyui_chart::Locale::parse(locale.as_deref().unwrap_or("en")),
    ))
}
