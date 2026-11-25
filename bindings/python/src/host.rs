use polyui_chart::{ChartOptions, ChartSpec};
use polyui_core::catalog::EMBED_ID;
use polyui_core::embed::{self, UiResource};
use polyui_core::pointer;
use polyui_core::protocol::{Close, Component, DataUpdate, Message, Open, Patch};
use polyui_core::tools::{ToolOptions, ToolProfile};
use polyui_core::validate::{
    self, Location, Operation, Problem, Problems, SealedAction, Surface, Update, Validator,
};
use polyui_core::{
    ActionSpec, ActionTable, CatalogError, CatalogSet, Limits, Locale, PropKind, PropSpec,
    RefResolver,
};
use serde_json::{Map, Value, json};

pub const CHART_TYPE: &str = "chart:Chart";
pub const SCENE_PROP: &str = "scene";
pub const DEFAULT_SURFACE: &str = "s1";

pub type ReadApp<'a> = dyn FnMut(&str) -> Option<UiResource> + 'a;

pub enum Input {
    Text(String),
    Json(Value),
}

pub type RefSource = (String, PropKind);

pub struct SourceRefs<'a, F> {
    sources: &'a [RefSource],
    lookup: F,
}

impl<'a, F> SourceRefs<'a, F>
where
    F: Fn(&str, &str) -> Result<Value, String>,
{
    pub fn new(sources: &'a [RefSource], lookup: F) -> Self {
        Self { sources, lookup }
    }
}

impl<F> RefResolver for SourceRefs<'_, F>
where
    F: Fn(&str, &str) -> Result<Value, String>,
{
    fn source_kind(&self, source: &str) -> Option<PropKind> {
        self.sources
            .iter()
            .find(|(name, _)| name == source)
            .map(|(_, kind)| kind.clone())
    }

    fn resolve(&self, source: &str, key: &str) -> Result<Value, String> {
        (self.lookup)(source, key)
    }
}

pub fn given_ref(given: &Map<String, Value>, source: &str, key: &str) -> Result<Value, String> {
    match given.get(&format!("{source}:{key}")) {
        None | Some(Value::Null) => Err("no value".into()),
        Some(value) => Ok(value.clone()),
    }
}

pub fn tool_options(strict: bool, profile: Option<&str>) -> Result<ToolOptions, String> {
    let name = profile.unwrap_or(ToolProfile::Compact.name());
    let profile = ToolProfile::from_name(name)
        .ok_or_else(|| format!("profile must be \"compact\" or \"full\", not {name:?}"))?;
    if strict && profile == ToolProfile::Compact {
        return Err("strict applies to the full profile; pass profile=\"full\" with it".into());
    }
    Ok(ToolOptions { strict, profile })
}

pub fn unresolved(source: &str, key: &str) -> Result<Value, String> {
    Err(format!(
        "$ref \"{source}:{key}\" is not resolved while validating"
    ))
}

pub fn action_table(spec: &Value) -> Result<ActionTable, CatalogError> {
    let mut table = ActionTable::new();
    match spec {
        Value::Null => Ok(table),
        Value::Object(entries) => {
            for (name, entry) in entries {
                let action = ActionSpec::from_value(entry).map_err(|error| CatalogError {
                    path: if error.path.is_empty() {
                        name.clone()
                    } else {
                        format!("{name}.{}", error.path)
                    },
                    message: error.message,
                })?;
                table.register(name, action);
            }
            Ok(table)
        }
        _ => Err(CatalogError {
            path: String::new(),
            message: "actions must be an object that maps action names to specs".into(),
        }),
    }
}

pub fn ref_sources(spec: &Value) -> Result<Vec<RefSource>, CatalogError> {
    let entries = match spec {
        Value::Null => return Ok(Vec::new()),
        Value::Object(entries) => entries,
        _ => {
            return Err(CatalogError {
                path: String::new(),
                message: "ref sources must be an object that maps source names to prop specs"
                    .into(),
            });
        }
    };
    entries
        .iter()
        .map(|(name, entry)| {
            let spec = match entry {
                Value::String(kind) => json!({ "kind": kind }),
                other => other.clone(),
            };
            PropSpec::from_value(&spec)
                .map(|parsed| (name.clone(), parsed.kind))
                .map_err(|error| CatalogError {
                    path: if error.path.is_empty() {
                        name.clone()
                    } else {
                        format!("{name}.{}", error.path)
                    },
                    message: error.message,
                })
        })
        .collect()
}

pub struct Settings {
    pub catalogs: CatalogSet,
    pub actions: ActionTable,
    pub sources: Vec<RefSource>,
    pub allow_http: bool,
}

impl Settings {
    pub fn ref_keys(&self, input: &Input) -> Vec<String> {
        let parsed;
        let payload = match input {
            Input::Json(value) => value,
            Input::Text(text) => match serde_json::from_str(text) {
                Ok(value) => {
                    parsed = value;
                    &parsed
                }
                Err(_) => return Vec::new(),
            },
        };
        validate::ref_keys(payload)
            .into_iter()
            .filter(|reference| {
                validate::split_ref(reference)
                    .is_some_and(|(source, _)| self.sources.iter().any(|(name, _)| name == source))
            })
            .collect()
    }

    pub fn validator<'a>(&'a self, refs: &'a dyn RefResolver) -> Validator<'a> {
        Validator::new(&self.catalogs, &self.actions, refs).allow_http(self.allow_http)
    }

    pub fn show_ui(&self, input: &Input) -> Result<Surface, Problems> {
        let refs = SourceRefs::new(&self.sources, unresolved);
        let validator = self.validator(&refs);
        match input {
            Input::Text(text) => validator.show_ui_str(text),
            Input::Json(value) => validator.show_ui(value),
        }
    }

    pub fn update_ui(&self, input: &Input, current: &Surface) -> Result<Update, Problems> {
        let refs = SourceRefs::new(&self.sources, unresolved);
        let validator = self.validator(&refs);
        match input {
            Input::Text(text) => validator.update_ui_str(text, current),
            Input::Json(value) => validator.update_ui(value, current),
        }
    }

    pub fn close_ui(&self, input: &Input) -> Result<String, Problems> {
        let refs = SourceRefs::new(&self.sources, unresolved);
        let validator = self.validator(&refs);
        match input {
            Input::Text(text) => validator.close_ui_str(text),
            Input::Json(value) => validator.close_ui(value),
        }
    }

    pub fn resolve(
        &self,
        components: &[Component],
        refs: &dyn RefResolver,
        operation: Operation,
    ) -> Result<Vec<Component>, Problems> {
        self.validator(refs).resolve_refs(components, operation)
    }

    pub fn seal(
        &self,
        components: &mut [Component],
        next_id: &mut dyn FnMut() -> String,
    ) -> Vec<SealedAction> {
        validate::seal_actions(components, &self.catalogs, &self.actions, next_id)
    }

    pub fn client_view(
        &self,
        surface: &Surface,
        declared: Option<&[String]>,
        locale: Locale,
    ) -> Surface {
        match declared {
            Some(declared) => validate::downgrade_for_client(
                surface,
                &self.catalogs,
                &self.catalogs.restrict(declared),
                locale,
            ),
            None => surface.clone(),
        }
    }
}

pub struct View<'a> {
    pub declared: Option<&'a [String]>,
    pub locale_tag: &'a str,
    pub chart: ChartOptions,
}

impl<'a> View<'a> {
    pub fn new(declared: Option<&'a [String]>, locale_tag: &'a str) -> Self {
        Self {
            declared,
            locale_tag,
            chart: chart_options(locale_tag, None, None),
        }
    }

    pub fn locale(&self) -> Locale {
        Locale::from_tag(self.locale_tag)
    }

    pub fn draws_embeds(&self) -> bool {
        self.declared.is_none_or(|declared| {
            declared
                .iter()
                .any(|tag| tag.split_once('@').is_some_and(|(id, _)| id == EMBED_ID))
        })
    }
}

pub struct Prepared {
    pub surface_id: String,
    pub model: Surface,
    pub client: Surface,
    pub messages: Vec<Value>,
    pub summary: String,
    pub actions: Vec<SealedAction>,
    pub seq: u64,
    pub touched: Vec<String>,
}

pub fn chart_options(locale_tag: &str, width: Option<u32>, height: Option<u32>) -> ChartOptions {
    let defaults = ChartOptions::with_locale(polyui_chart::Locale::parse(locale_tag));
    ChartOptions {
        width: width.unwrap_or(defaults.width),
        height: height.unwrap_or(defaults.height),
        ..defaults
    }
}

fn literal_props(props: &Map<String, Value>, data: &Value) -> Map<String, Value> {
    props
        .iter()
        .filter_map(|(name, value)| {
            let bound = value
                .as_object()
                .filter(|map| map.len() == 1)
                .and_then(|map| map.get("$bind"))
                .and_then(Value::as_str);
            match bound {
                Some(path) => pointer::get(data, path).map(|found| (name.clone(), found.clone())),
                None => Some((name.clone(), value.clone())),
            }
        })
        .collect()
}

pub fn chart_spec(props: Value) -> Result<ChartSpec, String> {
    serde_json::from_value::<ChartSpec>(props).map_err(|error| error.to_string())
}

pub fn attach_scenes(
    components: &mut [Component],
    data: &Value,
    list: &str,
    options: &ChartOptions,
) -> Vec<Problem> {
    let mut problems = Vec::new();
    for (index, component) in components.iter_mut().enumerate() {
        if component.type_name != CHART_TYPE {
            continue;
        }
        let location = Location::Item {
            list: list.to_string(),
            index,
            id: Some(component.id.clone()),
            prop: None,
        };
        let spec = match chart_spec(Value::Object(literal_props(&component.props, data))) {
            Ok(spec) => spec,
            Err(error) => {
                problems.push(Problem::new(
                    location,
                    format!("chart props could not be read ({error}). Write title, kind and rows inline."),
                ));
                continue;
            }
        };
        let scene = polyui_chart::compile(&spec, options)
            .map_err(|error| error.to_string())
            .and_then(|scene| serde_json::to_value(&scene).map_err(|error| error.to_string()));
        match scene {
            Ok(scene) => {
                component.props.insert(SCENE_PROP.into(), scene);
                if component.say.is_none() {
                    component.say = Some(polyui_chart::describe(&spec, options.locale));
                }
            }
            Err(error) => problems.push(Problem::new(
                location,
                format!("chart could not be drawn: {error}."),
            )),
        }
    }
    problems
}

pub fn attach_apps(
    components: &mut [Component],
    list: &str,
    read: &mut ReadApp<'_>,
) -> Vec<Problem> {
    embed::attach_apps(components, list, &Limits::default(), read)
}

fn fail(operation: Operation, items: Vec<Problem>) -> Result<(), Problems> {
    if items.is_empty() {
        Ok(())
    } else {
        Err(Problems { operation, items })
    }
}

fn data_problem(operation: Operation, error: impl std::fmt::Display) -> Problems {
    Problems {
        operation,
        items: vec![Problem::new(
            Location::Field("data".into()),
            format!("{error}."),
        )],
    }
}

pub fn prepare_show(
    settings: &Settings,
    input: &Input,
    refs: &dyn RefResolver,
    surface_id: Option<&str>,
    view: &View<'_>,
    next_id: &mut dyn FnMut() -> String,
    read: &mut ReadApp<'_>,
) -> Result<Prepared, Problems> {
    let mut model = settings.show_ui(input)?;
    let mut client = model.clone();
    client.components = settings.resolve(&model.components, refs, Operation::Show)?;
    let data = client.data_value();
    let mut problems = attach_scenes(&mut client.components, &data, "components", &view.chart);
    if view.draws_embeds() {
        problems.extend(attach_apps(&mut client.components, "components", read));
    }
    fail(Operation::Show, problems)?;
    let id = model
        .id
        .clone()
        .or_else(|| surface_id.map(str::to_string))
        .unwrap_or_else(|| DEFAULT_SURFACE.to_string());
    let actions = settings.seal(&mut client.components, next_id);
    model.id = Some(id.clone());
    client.id = Some(id.clone());
    let locale = view.locale();
    let shown = settings.client_view(&client, view.declared, locale);
    let fallback = client.fallback(&settings.catalogs, locale);
    let summary = model.summary(&id, &settings.catalogs, locale);
    let message = open_message(&id, &shown, 1, Some(fallback));
    Ok(Prepared {
        surface_id: id,
        model,
        client,
        messages: vec![message],
        summary,
        actions,
        seq: 1,
        touched: Vec::new(),
    })
}

pub struct Current<'a> {
    pub model: &'a Surface,
    pub client: &'a Surface,
    pub seq: u64,
}

pub fn prepare_update(
    settings: &Settings,
    input: &Input,
    current: Current<'_>,
    refs: &dyn RefResolver,
    view: &View<'_>,
    next_id: &mut dyn FnMut() -> String,
    read: &mut ReadApp<'_>,
) -> Result<Prepared, Problems> {
    let update = settings.update_ui(input, current.model)?;
    if let Some(open) = current.model.id.as_deref()
        && open != update.surface
    {
        return Err(Problems {
            operation: Operation::Update,
            items: vec![Problem::new(
                Location::Field("surface".into()),
                format!("\"{}\" is not open. Open surfaces: {open}.", update.surface),
            )],
        });
    }
    let mut upsert = settings.resolve(&update.upsert, refs, Operation::Update)?;
    let mut data_after = current.client.clone();
    data_after
        .apply(&Update {
            surface: update.surface.clone(),
            data: update.data.clone(),
            ..Update::default()
        })
        .map_err(|error| data_problem(Operation::Update, error))?;
    let mut problems = attach_scenes(&mut upsert, &data_after.data_value(), "upsert", &view.chart);
    if view.draws_embeds() {
        problems.extend(attach_apps(&mut upsert, "upsert", read));
    }
    fail(Operation::Update, problems)?;
    let actions = settings.seal(&mut upsert, next_id);
    let mut model = current.model.clone();
    let mut client = current.client.clone();
    let client_update = Update {
        upsert,
        ..update.clone()
    };
    model
        .apply(&update)
        .and_then(|()| client.apply(&client_update))
        .map_err(|error| data_problem(Operation::Update, error))?;
    let id = update.surface.clone();
    model.id = Some(id.clone());
    client.id = Some(id.clone());
    let locale = view.locale();
    let before = settings.client_view(current.client, view.declared, locale);
    let after = settings.client_view(&client, view.declared, locale);
    let changed: Vec<Component> = after
        .components
        .iter()
        .filter(|component| !before.components.contains(component))
        .cloned()
        .collect();
    let touched: Vec<String> = update
        .upsert
        .iter()
        .map(|component| component.id.clone())
        .chain(update.remove.iter().cloned())
        .collect();
    let mut seq = current.seq;
    let mut messages = Vec::new();
    if !update.data.is_empty() {
        seq += 1;
        messages.push(
            Message::Data(DataUpdate {
                surface: id.clone(),
                seq,
                ops: update.data.clone(),
            })
            .to_value(),
        );
    }
    if !changed.is_empty() || !update.remove.is_empty() || update.root.is_some() {
        seq += 1;
        messages.push(patch_message(
            &id,
            seq,
            changed,
            update.remove.clone(),
            update.root.clone(),
        ));
    }
    let summary = model.summary(&id, &settings.catalogs, locale);
    Ok(Prepared {
        surface_id: id,
        model,
        client,
        messages,
        summary,
        actions,
        seq,
        touched,
    })
}

pub fn open_message(
    surface_id: &str,
    surface: &Surface,
    seq: u64,
    fallback: Option<String>,
) -> Value {
    Message::Open(Open {
        surface: surface_id.to_string(),
        seq,
        root: surface.root.clone(),
        components: surface.components.clone(),
        data: surface.data.clone(),
        fallback,
    })
    .to_value()
}

pub fn patch_message(
    surface_id: &str,
    seq: u64,
    upsert: Vec<Component>,
    remove: Vec<String>,
    root: Option<String>,
) -> Value {
    Message::Patch(Patch {
        surface: surface_id.to_string(),
        seq,
        upsert,
        remove,
        root,
    })
    .to_value()
}

pub fn close_message(surface_id: &str, seq: u64) -> Value {
    Message::Close(Close {
        surface: surface_id.to_string(),
        seq,
    })
    .to_value()
}

pub fn random_id(prefix: &str) -> String {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).expect("the system random source failed");
    bytes.iter().fold(String::from(prefix), |mut out, byte| {
        out.push_str(&format!("{byte:02x}"));
        out
    })
}

pub fn action_id() -> String {
    random_id("act_")
}

pub fn surface_value(surface: &Surface) -> Value {
    json!({
        "id": surface.id,
        "root": surface.root,
        "components": surface.components,
        "data": surface.data,
    })
}

pub fn surface_from_value(value: Value) -> Result<Surface, String> {
    let Value::Object(mut map) = value else {
        return Err("a surface must be an object with root, components and data".into());
    };
    let id = match map.remove("id") {
        None | Some(Value::Null) => None,
        Some(Value::String(id)) => Some(id),
        Some(_) => return Err("surface id must be a string".into()),
    };
    let root = match map.remove("root") {
        Some(Value::String(root)) => root,
        _ => return Err("surface root must be a string".into()),
    };
    let components = serde_json::from_value::<Vec<Component>>(
        map.remove("components").unwrap_or(Value::Array(Vec::new())),
    )
    .map_err(|error| format!("surface components are not valid: {error}"))?;
    let data = match map.remove("data") {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(data)) => data,
        Some(_) => return Err("surface data must be an object".into()),
    };
    Ok(Surface {
        id,
        root,
        components,
        data,
    })
}

pub fn update_value(update: &Update) -> Value {
    json!({
        "surface": update.surface,
        "upsert": update.upsert,
        "remove": update.remove,
        "root": update.root,
        "data": update.data,
    })
}

pub fn sealed_value(action: &SealedAction) -> Value {
    json!({
        "action_id": action.action_id,
        "component": action.component,
        "prop": action.prop,
        "action": action.action,
        "args": action.args,
        "with": action.with,
        "requires_confirm": action.requires_confirm,
    })
}

pub fn problem_value(problem: &Problem) -> Value {
    let mut out = Map::new();
    out.insert("at".into(), Value::from(problem.location.to_string()));
    out.insert("message".into(), Value::from(problem.message.clone()));
    let mut put = |key: &str, value: Option<Value>| {
        if let Some(value) = value {
            out.insert(key.into(), value);
        }
    };
    match &problem.location {
        Location::Input => {}
        Location::Field(name) => put("field", Some(Value::from(name.clone()))),
        Location::Item {
            list,
            index,
            id,
            prop,
        } => {
            put("list", Some(Value::from(list.clone())));
            put("index", Some(Value::from(*index)));
            put("id", id.clone().map(Value::from));
            put("prop", prop.clone().map(Value::from));
        }
        Location::Entry { list, index, path } => {
            put("list", Some(Value::from(list.clone())));
            put("index", Some(Value::from(*index)));
            put("path", path.clone().map(Value::from));
        }
        Location::Existing { id, prop } => {
            put("id", Some(Value::from(id.clone())));
            put("prop", prop.clone().map(Value::from));
        }
    }
    Value::Object(out)
}

pub fn operation_name(operation: Operation) -> &'static str {
    match operation {
        Operation::Show => "show",
        Operation::Update => "update",
        Operation::Close => "close",
    }
}

pub fn problems_value(problems: &Problems) -> Value {
    Value::Array(problems.items.iter().map(problem_value).collect())
}
