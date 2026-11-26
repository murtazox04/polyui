use polyui_chart::{ChartOptions, ChartSpec};
use polyui_core::catalog::EMBED_ID;
use polyui_core::embed::{self, UiResource};
use polyui_core::pointer;
use polyui_core::protocol::{Close, Component, DataOp, DataUpdate, Message, Open, Patch};
use polyui_core::stream::{ShowUiStream, StreamEvent};
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
        let Some(payload) = parsed_payload(input) else {
            return Vec::new();
        };
        validate::ref_keys(&payload)
            .into_iter()
            .filter(|reference| {
                validate::split_ref(reference)
                    .is_some_and(|(source, _)| self.sources.iter().any(|(name, _)| name == source))
            })
            .collect()
    }

    pub fn resource_uris(&self, input: &Input) -> Vec<String> {
        let Some(payload) = parsed_payload(input) else {
            return Vec::new();
        };
        let mut uris = embed::resources_in(&payload, "components");
        for uri in embed::resources_in(&payload, "upsert") {
            if !uris.contains(&uri) {
                uris.push(uri);
            }
        }
        uris
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

fn parsed_payload(input: &Input) -> Option<std::borrow::Cow<'_, Value>> {
    match input {
        Input::Json(value) => Some(std::borrow::Cow::Borrowed(value)),
        Input::Text(text) => serde_json::from_str(text).ok().map(std::borrow::Cow::Owned),
    }
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

pub struct ShowStream {
    stream: ShowUiStream,
    dead: bool,
    named: Option<String>,
    root: Option<String>,
    model: Vec<Component>,
    prepared: Vec<Component>,
    actions: Vec<SealedAction>,
    opened: Option<Opened>,
}

#[derive(Clone)]
struct Opened {
    client: Surface,
    seq: u64,
}

impl Default for ShowStream {
    fn default() -> Self {
        Self {
            stream: ShowUiStream::new(Limits::default()),
            dead: false,
            named: None,
            root: None,
            model: Vec::new(),
            prepared: Vec::new(),
            actions: Vec::new(),
            opened: None,
        }
    }
}

fn reachable(root: &str, components: &[Component]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    let mut pending = vec![root.to_string()];
    while let Some(id) = pending.pop() {
        if seen.contains(&id) {
            continue;
        }
        if let Some(component) = components.iter().find(|component| component.id == id) {
            pending.extend(component.children.iter().cloned());
        }
        seen.push(id);
    }
    seen
}

fn data_changes(before: &Map<String, Value>, after: &Map<String, Value>) -> Vec<DataOp> {
    let removed = before
        .keys()
        .filter(|key| !after.contains_key(*key))
        .map(|key| DataOp::Remove {
            path: format!("/{}", pointer::escape_token(key)),
        });
    let changed = after
        .iter()
        .filter(|(key, value)| before.get(*key) != Some(*value))
        .map(|(key, value)| DataOp::Set {
            path: format!("/{}", pointer::escape_token(key)),
            value: value.clone(),
        });
    removed.chain(changed).collect()
}

pub struct StreamContext<'a, 'b> {
    pub settings: &'a Settings,
    pub refs: &'a dyn RefResolver,
    pub surface_id: Option<&'a str>,
    pub view: &'a View<'a>,
    pub next_id: &'b mut dyn FnMut() -> String,
    pub read: &'b mut ReadApp<'b>,
}

impl ShowStream {
    pub fn opened(&self) -> Option<&str> {
        self.opened
            .as_ref()
            .and_then(|opened| opened.client.id.as_deref())
    }

    pub fn seq(&self) -> u64 {
        self.opened.as_ref().map_or(0, |opened| opened.seq)
    }

    pub fn stopped(&self) -> bool {
        self.dead
    }

    pub fn text(&self) -> &str {
        self.stream.text()
    }

    pub fn close_message(&self) -> Option<Value> {
        let opened = self.opened.as_ref()?;
        let id = opened.client.id.as_deref()?;
        Some(close_message(id, opened.seq + 1))
    }

    pub fn feed(&mut self, context: &mut StreamContext<'_, '_>, chunk: &str) -> Vec<Value> {
        if self.dead {
            return Vec::new();
        }
        let validator = context.settings.validator(context.refs);
        let mut fresh = Vec::new();
        for event in self.stream.feed(&validator, chunk) {
            match event {
                StreamEvent::Surface(id) => self.named = Some(id),
                StreamEvent::Root(id) => self.root = Some(id),
                StreamEvent::Component { component, .. } => fresh.push(component),
                StreamEvent::Rejected { .. } | StreamEvent::Invalid(_) => {
                    self.dead = true;
                    return Vec::new();
                }
                StreamEvent::Data(_) => {}
            }
        }
        if !fresh.is_empty() && !self.accept(context, fresh) {
            self.dead = true;
            return Vec::new();
        }
        self.publish(context)
    }

    fn accept(&mut self, context: &mut StreamContext<'_, '_>, fresh: Vec<Component>) -> bool {
        let Ok(mut prepared) = context
            .settings
            .resolve(&fresh, context.refs, Operation::Show)
        else {
            return false;
        };
        let empty = Value::Object(Map::new());
        let mut problems = attach_scenes(&mut prepared, &empty, "components", &context.view.chart);
        if context.view.draws_embeds() {
            problems.extend(attach_apps(&mut prepared, "components", context.read));
        }
        if !problems.is_empty() {
            return false;
        }
        let sealed = context.settings.seal(&mut prepared, context.next_id);
        self.model.extend(fresh);
        self.prepared.extend(prepared);
        self.actions.extend(sealed);
        true
    }

    fn publish(&mut self, context: &StreamContext<'_, '_>) -> Vec<Value> {
        let locale = context.view.locale();
        let Some(opened) = self.opened.as_mut() else {
            let Some(root) = self.root.clone() else {
                return Vec::new();
            };
            if !self.model.iter().any(|component| component.id == root) {
                return Vec::new();
            }
            let id = self
                .named
                .clone()
                .or_else(|| context.surface_id.map(str::to_string))
                .unwrap_or_else(|| DEFAULT_SURFACE.to_string());
            let shown = reachable(&root, &self.prepared);
            let client = Surface {
                id: Some(id.clone()),
                root,
                components: self
                    .prepared
                    .iter()
                    .filter(|component| shown.contains(&component.id))
                    .cloned()
                    .collect(),
                data: Map::new(),
            };
            let view = context
                .settings
                .client_view(&client, context.view.declared, locale);
            let fallback = client.fallback(&context.settings.catalogs, locale);
            let message = open_message(&id, &view, 1, Some(fallback));
            self.opened = Some(Opened { client, seq: 1 });
            return vec![message];
        };
        let fresh: Vec<Component> = self
            .prepared
            .iter()
            .filter(|component| opened.client.get(&component.id).is_none())
            .cloned()
            .collect();
        if fresh.is_empty() {
            return Vec::new();
        }
        opened.client.components.extend(fresh.iter().cloned());
        let view = context
            .settings
            .client_view(&opened.client, context.view.declared, locale);
        let upsert: Vec<Component> = view
            .components
            .iter()
            .filter(|component| fresh.iter().any(|added| added.id == component.id))
            .cloned()
            .collect();
        opened.seq += 1;
        let id = opened.client.id.clone().unwrap_or_default();
        vec![patch_message(&id, opened.seq, upsert, Vec::new(), None)]
    }

    pub fn finish(
        &self,
        context: &mut StreamContext<'_, '_>,
        input: Option<&Input>,
    ) -> Result<Prepared, Problems> {
        let streamed_text;
        let input = match input {
            Some(input) => input,
            None => {
                streamed_text = Input::Text(self.stream.text().to_string());
                &streamed_text
            }
        };
        let Some(opened) = self.opened.clone() else {
            return prepare_show(
                context.settings,
                input,
                context.refs,
                context.surface_id,
                context.view,
                context.next_id,
                context.read,
            );
        };
        let mut model = context.settings.show_ui(input)?;
        let id = opened.client.id.clone().unwrap_or_default();
        if model.id.as_deref().is_some_and(|named| named != id) {
            return Err(Problems {
                operation: Operation::Show,
                items: vec![Problem::new(
                    Location::Field("surface".into()),
                    format!("The surface id changed from \"{id}\" while streaming."),
                )],
            });
        }
        let data = model.data_value();
        let mut client = model.clone();
        let mut actions = Vec::new();
        let mut positions = Vec::new();
        let mut fresh = Vec::new();
        for (index, component) in model.components.iter().enumerate() {
            let earlier = self
                .model
                .iter()
                .position(|seen| seen == component)
                .filter(|_| component.type_name != CHART_TYPE || model.data.is_empty());
            match earlier {
                Some(at) => {
                    client.components[index] = self.prepared[at].clone();
                    actions.extend(
                        self.actions
                            .iter()
                            .filter(|action| action.component == component.id)
                            .cloned(),
                    );
                }
                None => {
                    positions.push(index);
                    fresh.push(component.clone());
                }
            }
        }
        if !fresh.is_empty() {
            let mut prepared = context
                .settings
                .resolve(&fresh, context.refs, Operation::Show)
                .map_err(|problems| reindex(problems, &positions))?;
            let mut problems =
                attach_scenes(&mut prepared, &data, "components", &context.view.chart);
            if context.view.draws_embeds() {
                problems.extend(attach_apps(&mut prepared, "components", context.read));
            }
            fail(Operation::Show, problems).map_err(|problems| reindex(problems, &positions))?;
            actions.extend(context.settings.seal(&mut prepared, context.next_id));
            for (index, component) in positions.into_iter().zip(prepared) {
                client.components[index] = component;
            }
        }
        model.id = Some(id.clone());
        client.id = Some(id.clone());
        let locale = context.view.locale();
        let before = context
            .settings
            .client_view(&opened.client, context.view.declared, locale);
        let after = context
            .settings
            .client_view(&client, context.view.declared, locale);
        let mut seq = opened.seq;
        let mut messages = Vec::new();
        let ops = data_changes(&before.data, &after.data);
        if !ops.is_empty() {
            seq += 1;
            messages.push(
                Message::Data(DataUpdate {
                    surface: id.clone(),
                    seq,
                    ops,
                })
                .to_value(),
            );
        }
        let upsert: Vec<Component> = after
            .components
            .iter()
            .filter(|component| before.get(&component.id) != Some(*component))
            .cloned()
            .collect();
        let remove: Vec<String> = before
            .components
            .iter()
            .filter(|component| after.get(&component.id).is_none())
            .map(|component| component.id.clone())
            .collect();
        let root = (after.root != before.root).then(|| after.root.clone());
        if !upsert.is_empty() || !remove.is_empty() || root.is_some() {
            seq += 1;
            messages.push(patch_message(&id, seq, upsert, remove, root));
        }
        let summary = model.summary(&id, &context.settings.catalogs, locale);
        Ok(Prepared {
            surface_id: id,
            model,
            client,
            messages,
            summary,
            actions,
            seq,
            touched: Vec::new(),
        })
    }
}

fn reindex(mut problems: Problems, positions: &[usize]) -> Problems {
    for problem in &mut problems.items {
        if let Location::Item { index, .. } = &mut problem.location
            && let Some(position) = positions.get(*index)
        {
            *index = *position;
        }
    }
    problems
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
