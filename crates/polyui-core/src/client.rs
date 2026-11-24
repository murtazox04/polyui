use crate::catalog::{CatalogSet, PropKind, TypeDef};
use crate::decode;
use crate::limits::{self, Limits};
use crate::pointer;
use crate::protocol::{
    self, Action, ActionSource, Component, Confirm, DataOp, ErrorReason, Hello, Incoming, Message,
    ProtocolError,
};
use crate::text::{self, Locale};
use crate::view::{self, RefMode, TextKind};
use crate::voice::{self, Ordinal, Utterance, VoiceMatch};
use serde::Serialize;
use serde_json::{Map, Value};

pub const MAX_DETAIL_BYTES: usize = 4096;
pub const MAX_SCENE_ITEMS: usize = 20_000;
pub const MAX_SCENE_STRING_CHARS: usize = 16_384;
pub(crate) const CHART_TYPE: &str = "chart:Chart";
pub(crate) const SCENE_PROP: &str = "scene";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct A11y {
    pub role: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum RenderOp {
    Create {
        id: String,
        #[serde(rename = "type")]
        type_name: String,
        props: Map<String, Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<Value>,
        a11y: A11y,
    },
    Update {
        id: String,
        props: Map<String, Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<Value>,
        a11y: A11y,
    },
    Children {
        id: String,
        children: Vec<String>,
    },
    Remove {
        id: String,
    },
    Root {
        id: String,
    },
    Confirm {
        action_id: String,
        nonce: String,
        title: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        body: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        confirm_label: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        expires_ms: Option<u64>,
    },
    Highlight {
        id: Option<String>,
    },
    Close,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct RenderBatch {
    pub surface: String,
    pub ops: Vec<RenderOp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub send: Vec<Value>,
}

impl RenderBatch {
    pub fn empty(surface: &str) -> Self {
        Self {
            surface: surface.to_string(),
            ..Self::default()
        }
    }

    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }

    pub fn to_json(&self) -> String {
        self.to_value().to_string()
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Node {
    id: String,
    type_name: String,
    role: String,
    props: Map<String, Value>,
    value: Option<Value>,
    a11y: A11y,
    children: Vec<String>,
    say: String,
    fallback: bool,
    reachable: bool,
}

impl Node {
    fn create_op(&self) -> RenderOp {
        RenderOp::Create {
            id: self.id.clone(),
            type_name: self.type_name.clone(),
            props: self.props.clone(),
            value: self.value.clone(),
            a11y: self.a11y.clone(),
        }
    }

    fn update_op(&self) -> RenderOp {
        RenderOp::Update {
            id: self.id.clone(),
            props: self.props.clone(),
            value: self.value.clone(),
            a11y: self.a11y.clone(),
        }
    }

    fn children_op(&self) -> RenderOp {
        RenderOp::Children {
            id: self.id.clone(),
            children: self.children.clone(),
        }
    }

    fn disabled(&self) -> bool {
        self.props.get("disabled") == Some(&Value::Bool(true))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingConfirm {
    action_id: String,
    nonce: String,
}

#[derive(Debug, Clone, Default)]
struct SurfaceState {
    seq: u64,
    root: String,
    components: Vec<Component>,
    data: Value,
    nodes: Vec<Node>,
    rendered_root: Option<String>,
    fallback: String,
    awaiting_snapshot: bool,
    confirms: Vec<PendingConfirm>,
}

impl SurfaceState {
    fn component(&self, id: &str) -> Option<&Component> {
        self.components.iter().find(|component| component.id == id)
    }

    fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|node| node.id == id)
    }
}

enum Sequence {
    Apply,
    Duplicate,
    Gap,
    Unknown,
    Awaiting,
}

#[derive(Clone)]
struct Prepared<'a> {
    definition: Option<&'a TypeDef>,
    resolved: view::Resolved,
    values: Map<String, Value>,
}

struct Rendered {
    nodes: Vec<Node>,
    fallback: String,
    unknown_types: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct Surfaces(Vec<(String, SurfaceState)>);

impl Surfaces {
    fn slot(&self, surface: &str) -> Result<usize, usize> {
        self.0.binary_search_by(|(id, _)| id.as_str().cmp(surface))
    }

    fn get(&self, surface: &str) -> Option<&SurfaceState> {
        self.slot(surface).ok().map(|index| &self.0[index].1)
    }

    fn get_mut(&mut self, surface: &str) -> Option<&mut SurfaceState> {
        self.slot(surface).ok().map(|index| &mut self.0[index].1)
    }

    fn insert(&mut self, surface: String, state: SurfaceState) {
        match self.slot(&surface) {
            Ok(index) => self.0[index].1 = state,
            Err(index) => self.0.insert(index, (surface, state)),
        }
    }

    fn remove(&mut self, surface: &str) -> Option<SurfaceState> {
        self.slot(surface).ok().map(|index| self.0.remove(index).1)
    }

    fn keys(&self) -> impl Iterator<Item = &String> {
        self.0.iter().map(|(id, _)| id)
    }
}

#[derive(Debug, Clone)]
pub struct Client {
    hello: Hello,
    locale: Locale,
    catalogs: CatalogSet,
    limits: Limits,
    session: Option<String>,
    surfaces: Surfaces,
    reported_types: Vec<String>,
    allow_http: bool,
}

fn first_path_prop(definition: &TypeDef) -> Option<&str> {
    definition
        .props
        .iter()
        .find(|(_, spec)| spec.kind == PropKind::Path)
        .map(|(name, _)| name.as_str())
}

fn fallback_node(id: &str, text: &str, style: &str, reachable: bool) -> Node {
    let mut props = Map::new();
    props.insert("text".into(), Value::from(text));
    props.insert("style".into(), Value::from(style));
    Node {
        id: id.to_string(),
        type_name: "Text".into(),
        role: "text".into(),
        props,
        value: None,
        a11y: A11y {
            role: "text".into(),
            label: text.to_string(),
        },
        children: Vec::new(),
        say: text.to_string(),
        fallback: true,
        reachable,
    }
}

fn render(
    root: &str,
    components: &[Component],
    data: &Value,
    catalogs: &CatalogSet,
    locale: Locale,
    allow_http: bool,
) -> Rendered {
    let tree = view::build_tree(Some(root), components);
    let count = components.len();
    let mut alts = vec![String::new(); count];
    let mut prepared: Vec<Option<Prepared<'_>>> = vec![None; count];
    for &position in tree.order.iter().rev() {
        let component = &components[position];
        let definition = catalogs.get(&component.type_name);
        let resolved = definition
            .map(|definition| {
                view::resolve(component, definition, data, RefMode::Missing, allow_http)
            })
            .unwrap_or_default();
        let values = definition
            .map(|definition| view::template_values(definition, &resolved))
            .unwrap_or_default();
        let child_alts: Vec<String> = tree.children[position]
            .iter()
            .map(|&child| alts[child].clone())
            .collect();
        alts[position] = view::component_text(
            component,
            definition,
            &values,
            &child_alts,
            TextKind::Alt,
            locale,
        );
        prepared[position] = Some(Prepared {
            definition,
            resolved,
            values,
        });
    }
    let mut hidden = vec![false; count];
    let mut nodes = Vec::with_capacity(count);
    let mut unknown_types = Vec::new();
    for &position in &tree.order {
        let kids = &tree.children[position];
        let Some(Prepared {
            definition,
            resolved,
            values,
        }) = prepared[position].take()
        else {
            continue;
        };
        if hidden[position] {
            for &kid in kids {
                hidden[kid] = true;
            }
            continue;
        }
        let component = &components[position];
        let reachable = tree.reachable[position];
        let node = match definition {
            None => {
                unknown_types.push(component.type_name.clone());
                fallback_node(&component.id, &alts[position], "body", reachable)
            }
            Some(_) if resolved.missing_required => {
                fallback_node(&component.id, &alts[position], "muted", reachable)
            }
            Some(definition) => {
                let mut props = resolved.props;
                if definition.qualified == CHART_TYPE
                    && let Some(scene @ Value::Object(_)) = component.props.get(SCENE_PROP)
                {
                    props.insert(SCENE_PROP.into(), scene.clone());
                }
                let value = first_path_prop(definition)
                    .map(|name| resolved.values.get(name).cloned().unwrap_or(Value::Null));
                let label_alt = view::component_text(
                    component,
                    Some(definition),
                    &values,
                    &[],
                    TextKind::Alt,
                    locale,
                );
                let say = view::component_text(
                    component,
                    Some(definition),
                    &values,
                    &[],
                    TextKind::Say,
                    locale,
                );
                let children = if definition.has_children() {
                    kids.iter()
                        .map(|&kid| &components[kid])
                        .filter(|kid| {
                            catalogs.get(&kid.type_name).is_none_or(|kid_definition| {
                                definition.allows_child(&kid_definition.qualified)
                            })
                        })
                        .map(|kid| kid.id.clone())
                        .collect()
                } else {
                    Vec::new()
                };
                Node {
                    id: component.id.clone(),
                    type_name: definition.qualified.clone(),
                    role: definition.role.clone(),
                    props,
                    value,
                    a11y: A11y {
                        role: definition.role.clone(),
                        label: text::label_from_alt(&label_alt),
                    },
                    children,
                    say,
                    fallback: false,
                    reachable,
                }
            }
        };
        if node.fallback {
            for &kid in kids {
                hidden[kid] = true;
            }
        }
        nodes.push(node);
    }
    let fallback = view::position_of(components, root)
        .map(|position| alts[position].clone())
        .unwrap_or_default();
    Rendered {
        nodes,
        fallback,
        unknown_types,
    }
}

fn is_replaced(old: &[Node], node: &Node) -> bool {
    old.iter()
        .any(|previous| previous.id == node.id && previous.type_name != node.type_name)
}

fn diff(old: &[Node], new: &[Node]) -> Vec<RenderOp> {
    let mut replaced = Vec::new();
    let mut creates = Vec::new();
    let mut updates = Vec::new();
    let mut children = Vec::new();
    for node in new {
        let reattach = node.children.iter().any(|child| {
            new.iter()
                .any(|kid| kid.id == *child && is_replaced(old, kid))
        });
        match old.iter().find(|previous| previous.id == node.id) {
            None => {
                creates.push(node.create_op());
                if !node.children.is_empty() {
                    children.push(node.children_op());
                }
            }
            Some(previous) if previous.type_name != node.type_name => {
                replaced.push(RenderOp::Remove {
                    id: node.id.clone(),
                });
                creates.push(node.create_op());
                if !node.children.is_empty() {
                    children.push(node.children_op());
                }
            }
            Some(previous) => {
                if previous.props != node.props
                    || previous.value != node.value
                    || previous.a11y != node.a11y
                {
                    updates.push(node.update_op());
                }
                if previous.children != node.children || reattach {
                    children.push(node.children_op());
                }
            }
        }
    }
    let removes = old
        .iter()
        .filter(|node| !new.iter().any(|current| current.id == node.id))
        .map(|node| RenderOp::Remove {
            id: node.id.clone(),
        });
    replaced
        .into_iter()
        .chain(creates)
        .chain(updates)
        .chain(children)
        .chain(removes)
        .collect()
}

fn merge_components(components: &mut Vec<Component>, upsert: Vec<Component>, remove: &[String]) {
    components.retain(|component| !remove.contains(&component.id));
    for component in components.iter_mut() {
        component.children.retain(|child| !remove.contains(child));
    }
    for incoming in upsert {
        match components
            .iter_mut()
            .find(|existing| existing.id == incoming.id)
        {
            Some(slot) => *slot = incoming,
            None => components.push(incoming),
        }
    }
}

fn dedupe(components: Vec<Component>) -> Vec<Component> {
    let mut kept: Vec<Component> = Vec::with_capacity(components.len());
    for component in components {
        if !kept.iter().any(|existing| existing.id == component.id) {
            kept.push(component);
        }
    }
    kept
}

impl Client {
    pub fn new(hello_json: &str) -> Result<Self, ProtocolError> {
        Self::with_catalogs(hello_json, CatalogSet::builtin())
    }

    pub fn with_catalogs(
        hello_json: &str,
        mut catalogs: CatalogSet,
    ) -> Result<Self, ProtocolError> {
        let limits = Limits::default();
        let hello = decode::hello(hello_json, &limits)?;
        catalogs.keep_declared(&hello.catalogs);
        Ok(Self {
            locale: Locale::from_tag(hello.locale.as_deref().unwrap_or("en")),
            catalogs,
            hello,
            limits,
            session: None,
            surfaces: Surfaces::default(),
            reported_types: Vec::new(),
            allow_http: false,
        })
    }

    pub fn with_allow_http(mut self, allow: bool) -> Self {
        self.allow_http = allow;
        self
    }

    pub fn hello(&self) -> &Hello {
        &self.hello
    }

    pub fn locale(&self) -> Locale {
        self.locale
    }

    pub fn session(&self) -> Option<&str> {
        self.session.as_deref()
    }

    pub fn catalogs(&self) -> &CatalogSet {
        &self.catalogs
    }

    pub fn surfaces(&self) -> Vec<&str> {
        self.surfaces.keys().map(String::as_str).collect()
    }

    pub fn data(&self, surface: &str) -> Option<&Value> {
        self.surfaces.get(surface).map(|state| &state.data)
    }

    pub fn receive(&mut self, message: &str) -> RenderBatch {
        match decode::from_server(message, &self.limits) {
            Err(error) => RenderBatch {
                send: vec![protocol::error_value(
                    None,
                    ErrorReason::BadMessage,
                    Some(error.to_string()),
                )],
                ..RenderBatch::default()
            },
            Ok(Incoming::UnsupportedVersion {
                version,
                surface,
                fallback,
            }) => RenderBatch {
                surface: surface.clone().unwrap_or_default(),
                ops: Vec::new(),
                fallback,
                send: vec![protocol::error_value(
                    surface.as_deref(),
                    ErrorReason::UnsupportedVersion,
                    Some(format!("version {version} is not supported")),
                )],
            },
            Ok(Incoming::UnknownType { surface, .. }) => {
                RenderBatch::empty(surface.as_deref().unwrap_or_default())
            }
            Ok(Incoming::Message(message)) => self.handle(message),
        }
    }

    pub fn receive_json(&mut self, message: &str) -> String {
        self.receive(message).to_json()
    }

    fn handle(&mut self, message: Message) -> RenderBatch {
        match message {
            Message::Welcome(welcome) => {
                self.session = Some(welcome.session);
                if !welcome.catalogs.is_empty() {
                    self.catalogs.keep_declared(&welcome.catalogs);
                }
                RenderBatch::default()
            }
            Message::Open(open) => self.open(
                open.surface,
                open.seq,
                open.root,
                open.components,
                open.data,
            ),
            Message::Snapshot(snapshot) => self.open(
                snapshot.surface,
                snapshot.seq,
                snapshot.root,
                snapshot.components,
                snapshot.data,
            ),
            Message::Patch(patch) => self.patch(patch),
            Message::Data(update) => self.data_update(&update.surface, update.seq, &update.ops),
            Message::Close(close) => self.close(&close.surface, close.seq),
            Message::Sync(sync) => {
                let Some(state) = self.surfaces.get(&sync.surface) else {
                    return RenderBatch::empty(&sync.surface);
                };
                let known = sync
                    .highlight
                    .as_deref()
                    .is_none_or(|id| state.node(id).is_some());
                RenderBatch {
                    surface: sync.surface.clone(),
                    ops: if known {
                        vec![RenderOp::Highlight { id: sync.highlight }]
                    } else {
                        Vec::new()
                    },
                    ..RenderBatch::default()
                }
            }
            Message::Confirm(confirm) => self.confirm_request(confirm),
            Message::ActionResult(result) => {
                if let Some(state) = self.surfaces.get_mut(&result.surface) {
                    state
                        .confirms
                        .retain(|pending| pending.action_id != result.action_id);
                }
                RenderBatch::empty(&result.surface)
            }
            other => RenderBatch::empty(other.surface().unwrap_or_default()),
        }
    }

    fn confirm_request(&mut self, confirm: Confirm) -> RenderBatch {
        let Some(state) = self.surfaces.get_mut(&confirm.surface) else {
            return RenderBatch::empty(&confirm.surface);
        };
        state
            .confirms
            .retain(|pending| pending.action_id != confirm.action_id);
        state.confirms.push(PendingConfirm {
            action_id: confirm.action_id.clone(),
            nonce: confirm.nonce.clone(),
        });
        RenderBatch {
            surface: confirm.surface,
            ops: vec![RenderOp::Confirm {
                action_id: confirm.action_id,
                nonce: confirm.nonce,
                title: confirm.title,
                body: confirm.body,
                confirm_label: confirm.confirm_label,
                expires_ms: confirm.expires_ms,
            }],
            ..RenderBatch::default()
        }
    }

    fn sequence(&mut self, surface: &str, seq: u64) -> Sequence {
        let Some(state) = self.surfaces.get_mut(surface) else {
            return Sequence::Unknown;
        };
        if state.awaiting_snapshot {
            return Sequence::Awaiting;
        }
        if seq <= state.seq {
            return Sequence::Duplicate;
        }
        if seq != state.seq + 1 {
            state.awaiting_snapshot = true;
            return Sequence::Gap;
        }
        Sequence::Apply
    }

    fn not_applied(&mut self, surface: &str, outcome: Sequence) -> RenderBatch {
        match outcome {
            Sequence::Unknown => {
                self.surfaces.insert(
                    surface.to_string(),
                    SurfaceState {
                        awaiting_snapshot: true,
                        data: Value::Object(Map::new()),
                        ..SurfaceState::default()
                    },
                );
                self.resync_batch(surface)
            }
            Sequence::Gap => self.resync_batch(surface),
            Sequence::Apply | Sequence::Duplicate | Sequence::Awaiting => {
                RenderBatch::empty(surface)
            }
        }
    }

    fn resync_batch(&self, surface: &str) -> RenderBatch {
        RenderBatch {
            surface: surface.to_string(),
            send: vec![protocol::resync_value(surface)],
            ..RenderBatch::default()
        }
    }

    fn reject(&mut self, surface: &str, detail: String) -> RenderBatch {
        if let Some(state) = self.surfaces.get_mut(surface) {
            state.awaiting_snapshot = true;
        }
        RenderBatch {
            surface: surface.to_string(),
            send: vec![
                protocol::error_value(Some(surface), ErrorReason::BadMessage, Some(detail)),
                protocol::resync_value(surface),
            ],
            ..RenderBatch::default()
        }
    }

    fn check_data(&self, data: &Value) -> Result<(), String> {
        limits::check_data_model(data, &self.limits).map_err(|error| error.to_string())
    }

    fn open(
        &mut self,
        surface: String,
        seq: u64,
        root: String,
        components: Vec<Component>,
        data: Map<String, Value>,
    ) -> RenderBatch {
        let components = dedupe(components);
        if components.len() > self.limits.max_components {
            return self.reject(
                &surface,
                format!(
                    "{} components, max is {}",
                    components.len(),
                    self.limits.max_components
                ),
            );
        }
        let data = Value::Object(data);
        if let Err(detail) = self.check_data(&data) {
            return self.reject(&surface, detail);
        }
        let previous = self.surfaces.remove(&surface).unwrap_or_default();
        let state = SurfaceState {
            seq,
            root,
            components,
            data,
            ..SurfaceState::default()
        };
        self.commit(&surface, state, previous.nodes, previous.rendered_root)
    }

    fn patch(&mut self, patch: protocol::Patch) -> RenderBatch {
        let outcome = self.sequence(&patch.surface, patch.seq);
        if !matches!(outcome, Sequence::Apply) {
            return self.not_applied(&patch.surface, outcome);
        }
        let Some(state) = self.surfaces.get(&patch.surface) else {
            return RenderBatch::empty(&patch.surface);
        };
        let mut components = state.components.clone();
        merge_components(&mut components, dedupe(patch.upsert), &patch.remove);
        if components.len() > self.limits.max_components {
            return self.reject(
                &patch.surface,
                format!(
                    "{} components, max is {}",
                    components.len(),
                    self.limits.max_components
                ),
            );
        }
        let Some(mut state) = self.surfaces.remove(&patch.surface) else {
            return RenderBatch::empty(&patch.surface);
        };
        let old_nodes = std::mem::take(&mut state.nodes);
        let old_root = state.rendered_root.take();
        state.components = components;
        if let Some(root) = patch.root {
            state.root = root;
        }
        state.seq = patch.seq;
        self.commit(&patch.surface, state, old_nodes, old_root)
    }

    fn data_update(&mut self, surface: &str, seq: u64, ops: &[DataOp]) -> RenderBatch {
        let outcome = self.sequence(surface, seq);
        if !matches!(outcome, Sequence::Apply) {
            return self.not_applied(surface, outcome);
        }
        let Some(state) = self.surfaces.get(surface) else {
            return RenderBatch::empty(surface);
        };
        let mut data = state.data.clone();
        for op in ops {
            if let Err(error) = op.apply(&mut data) {
                return self.reject(surface, format!("{} at {}", error, op.path()));
            }
        }
        if let Err(detail) = self.check_data(&data) {
            return self.reject(surface, detail);
        }
        self.replace_data(surface, data, Some(seq))
    }

    fn replace_data(&mut self, surface: &str, data: Value, seq: Option<u64>) -> RenderBatch {
        let Some(mut state) = self.surfaces.remove(surface) else {
            return RenderBatch::empty(surface);
        };
        let old_nodes = std::mem::take(&mut state.nodes);
        let old_root = state.rendered_root.take();
        state.data = data;
        if let Some(seq) = seq {
            state.seq = seq;
        }
        self.commit(surface, state, old_nodes, old_root)
    }

    fn close(&mut self, surface: &str, seq: u64) -> RenderBatch {
        match self.surfaces.get(surface) {
            Some(state) if seq > state.seq || state.awaiting_snapshot => {
                self.surfaces.remove(surface);
                RenderBatch {
                    surface: surface.to_string(),
                    ops: vec![RenderOp::Close],
                    ..RenderBatch::default()
                }
            }
            _ => RenderBatch::empty(surface),
        }
    }

    fn commit(
        &mut self,
        surface: &str,
        mut state: SurfaceState,
        old_nodes: Vec<Node>,
        old_root: Option<String>,
    ) -> RenderBatch {
        let rendered = render(
            &state.root,
            &state.components,
            &state.data,
            &self.catalogs,
            self.locale,
            self.allow_http,
        );
        let mut ops = diff(&old_nodes, &rendered.nodes);
        let root_node = rendered.nodes.iter().find(|node| node.id == state.root);
        let root_replaced = root_node.is_some_and(|node| is_replaced(&old_nodes, node));
        let root_rendered = root_node.is_some();
        state.rendered_root = root_rendered.then(|| state.root.clone());
        if root_rendered && (root_replaced || old_root.as_deref() != Some(state.root.as_str())) {
            ops.push(RenderOp::Root {
                id: state.root.clone(),
            });
        }
        let mut send = Vec::new();
        for type_name in rendered.unknown_types {
            if !self.reported_types.contains(&type_name) {
                self.reported_types.push(type_name.clone());
                send.push(protocol::error_value(
                    Some(surface),
                    ErrorReason::UnknownComponent,
                    Some(type_name),
                ));
            }
        }
        state.nodes = rendered.nodes;
        state.fallback = rendered.fallback.clone();
        state.awaiting_snapshot = false;
        self.surfaces.insert(surface.to_string(), state);
        RenderBatch {
            surface: surface.to_string(),
            fallback: (!ops.is_empty()).then_some(rendered.fallback),
            ops,
            send,
        }
    }

    pub fn fallback(&self, surface: &str) -> Option<&str> {
        self.surfaces
            .get(surface)
            .map(|state| state.fallback.as_str())
    }

    pub fn set_value(&mut self, surface: &str, path: &str, value_json: &str) -> RenderBatch {
        let Some(value) = self.parse_value(value_json) else {
            return RenderBatch::empty(surface);
        };
        self.write_value(surface, path, value)
    }

    pub fn set_value_json(&mut self, surface: &str, path: &str, value_json: &str) -> String {
        self.set_value(surface, path, value_json).to_json()
    }

    fn parse_value(&self, value_json: &str) -> Option<Value> {
        limits::prescan(value_json, &self.limits).ok()?;
        let value: Value = serde_json::from_str(value_json).ok()?;
        let too_long = value
            .as_str()
            .is_some_and(|text| text.chars().count() > self.limits.max_string_chars);
        (!too_long).then_some(value)
    }

    fn write_value(&mut self, surface: &str, path: &str, value: Value) -> RenderBatch {
        let Some(state) = self.surfaces.get(surface) else {
            return RenderBatch::empty(surface);
        };
        if state.awaiting_snapshot || !pointer::is_valid_target(path) {
            return RenderBatch::empty(surface);
        }
        let mut data = state.data.clone();
        if pointer::set(&mut data, path, value).is_err() || self.check_data(&data).is_err() {
            return RenderBatch::empty(surface);
        }
        self.replace_data(surface, data, None)
    }

    pub fn fire(
        &self,
        surface: &str,
        component: &str,
        prop: &str,
        detail: Option<&str>,
    ) -> Option<Value> {
        self.fire_from(surface, component, prop, detail, ActionSource::Tap)
    }

    pub fn fire_json(
        &self,
        surface: &str,
        component: &str,
        prop: &str,
        detail_json: &str,
    ) -> String {
        let detail = Some(detail_json).filter(|text| !text.trim().is_empty());
        self.fire(surface, component, prop, detail)
            .unwrap_or(Value::Null)
            .to_string()
    }

    pub fn fire_from(
        &self,
        surface: &str,
        component: &str,
        prop: &str,
        detail: Option<&str>,
        source: ActionSource,
    ) -> Option<Value> {
        let detail = match detail.map(str::trim) {
            None | Some("") | Some("null") => None,
            Some(text) => {
                if text.len() > MAX_DETAIL_BYTES {
                    return None;
                }
                limits::prescan(text, &self.limits).ok()?;
                match serde_json::from_str::<Value>(text).ok()? {
                    Value::Object(map) => Some(map),
                    _ => return None,
                }
            }
        };
        let state = self.surfaces.get(surface)?;
        if state.awaiting_snapshot {
            return None;
        }
        let raw = state.component(component)?;
        let node = state.node(component)?;
        let definition = self.catalogs.get(&raw.type_name)?;
        if node.fallback || node.disabled() {
            return None;
        }
        if definition.prop(prop)?.kind != PropKind::Action {
            return None;
        }
        let raw_handle = raw.props.get(prop)?;
        let handle = view::handle(raw_handle)?;
        let action_id = handle.get("action_id")?.as_str()?.to_string();
        let mut paths = view::handle_paths(raw_handle);
        for (name, spec) in &definition.props {
            if spec.kind == PropKind::Path
                && let Some(path) = raw.props.get(name).and_then(Value::as_str)
                && pointer::is_valid_target(path)
                && !paths.iter().any(|existing| existing == path)
            {
                paths.push(path.to_string());
            }
        }
        let values: Map<String, Value> = paths
            .into_iter()
            .map(|path| {
                let current = pointer::get(&state.data, &path)
                    .cloned()
                    .unwrap_or(Value::Null);
                (path, current)
            })
            .collect();
        Some(protocol::action_value(&Action {
            surface: surface.to_string(),
            action_id,
            values,
            confirm: None,
            source,
            detail,
        }))
    }

    pub fn confirm(&mut self, surface: &str, action_id: &str, nonce: &str) -> Option<Value> {
        self.confirm_from(surface, action_id, nonce, ActionSource::Tap)
    }

    pub fn confirm_json(&mut self, surface: &str, action_id: &str, nonce: &str) -> String {
        self.confirm(surface, action_id, nonce)
            .unwrap_or(Value::Null)
            .to_string()
    }

    fn confirm_from(
        &mut self,
        surface: &str,
        action_id: &str,
        nonce: &str,
        source: ActionSource,
    ) -> Option<Value> {
        let state = self.surfaces.get_mut(surface)?;
        let position = state
            .confirms
            .iter()
            .position(|pending| pending.action_id == action_id && pending.nonce == nonce)?;
        state.confirms.remove(position);
        Some(protocol::action_value(&Action {
            surface: surface.to_string(),
            action_id: action_id.to_string(),
            values: Map::new(),
            confirm: Some(nonce.to_string()),
            source,
            detail: None,
        }))
    }

    pub fn say(&self, surface: &str) -> Vec<Utterance> {
        let Some(state) = self.surfaces.get(surface) else {
            return Vec::new();
        };
        state
            .nodes
            .iter()
            .filter(|node| node.reachable)
            .map(|node| Utterance {
                id: node.id.clone(),
                text: text::as_sentence(&node.say),
            })
            .filter(|utterance| !utterance.text.is_empty())
            .collect()
    }

    pub fn say_json(&self, surface: &str) -> String {
        serde_json::to_string(&self.say(surface)).unwrap_or_else(|_| "[]".into())
    }

    pub fn match_voice(&mut self, surface: &str, spoken: &str) -> Option<VoiceMatch> {
        if spoken.len() > self.limits.max_string_chars * 4 {
            return None;
        }
        let normalized = voice::normalize(spoken);
        let words = voice::tokens(&normalized);
        if words.is_empty() {
            return None;
        }
        let answer = voice::yes_no(&words);
        if let Some(found) = self.match_pending_confirm(surface, answer) {
            return Some(found);
        }
        if let Some(found) = self.match_dialog(surface, answer) {
            return Some(found);
        }
        self.match_targets(surface, &words)
    }

    pub fn match_voice_json(&mut self, surface: &str, spoken: &str) -> String {
        self.match_voice(surface, spoken)
            .and_then(|found| serde_json::to_value(found).ok())
            .unwrap_or(Value::Null)
            .to_string()
    }

    fn match_pending_confirm(&mut self, surface: &str, answer: Option<bool>) -> Option<VoiceMatch> {
        let answer = answer?;
        let state = self.surfaces.get(surface)?;
        let pending = state.confirms.last()?.clone();
        let component = state
            .components
            .iter()
            .find(|component| {
                component.props.values().any(|value| {
                    value.get("action_id").and_then(Value::as_str)
                        == Some(pending.action_id.as_str())
                })
            })
            .map_or_else(
                || pending.action_id.clone(),
                |component| component.id.clone(),
            );
        let action = if answer {
            self.confirm_from(
                surface,
                &pending.action_id,
                &pending.nonce,
                ActionSource::Voice,
            )
        } else {
            if let Some(state) = self.surfaces.get_mut(surface) {
                state.confirms.retain(|entry| entry != &pending);
            }
            None
        };
        Some(VoiceMatch {
            component,
            option: Some(if answer { "yes" } else { "no" }.to_string()),
            batch: RenderBatch::empty(surface),
            action,
        })
    }

    fn match_dialog(&self, surface: &str, answer: Option<bool>) -> Option<VoiceMatch> {
        let answer = answer?;
        let state = self.surfaces.get(surface)?;
        let node = state
            .nodes
            .iter()
            .find(|node| node.reachable && !node.fallback && node.role == "dialog")?;
        let prop = if answer { "on_confirm" } else { "on_cancel" };
        let action = self.fire_from(surface, &node.id, prop, None, ActionSource::Voice);
        Some(VoiceMatch {
            component: node.id.clone(),
            option: Some(if answer { "yes" } else { "no" }.to_string()),
            batch: RenderBatch::empty(surface),
            action,
        })
    }

    fn match_targets(&mut self, surface: &str, words: &[&str]) -> Option<VoiceMatch> {
        let state = self.surfaces.get(surface)?;
        if state.awaiting_snapshot {
            return None;
        }
        let spoken_ordinal = voice::ordinal(words);
        let mut best: Option<(usize, Target)> = None;
        let mut consider = |score: usize, target: Target| {
            if score > 0 && best.as_ref().is_none_or(|(current, _)| score > *current) {
                best = Some((score, target));
            }
        };
        let live: Vec<&Node> = state
            .nodes
            .iter()
            .filter(|node| node.reachable && !node.fallback)
            .collect();
        let choices: Vec<&Node> = live
            .iter()
            .copied()
            .filter(|node| {
                node.role == "radiogroup" && node.props.get("options").is_some_and(Value::is_array)
            })
            .collect();
        for (group_index, node) in choices.iter().enumerate() {
            let options = node
                .props
                .get("options")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for (position, option) in options.iter().enumerate() {
                let Some(option_id) = option.get("id").and_then(Value::as_str) else {
                    continue;
                };
                let label = option
                    .get("label")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let mut score =
                    voice::phrase_score(words, label).max(voice::phrase_score(words, option_id));
                if group_index == 0
                    && spoken_ordinal.and_then(|ordinal| ordinal.position(options.len()))
                        == Some(position)
                {
                    score = score.max(voice::ORDINAL_MATCH);
                }
                consider(
                    score,
                    Target::Option {
                        component: node.id.clone(),
                        option: option_id.to_string(),
                    },
                );
            }
        }
        let actionable: Vec<&Node> = live
            .iter()
            .copied()
            .filter(|node| node.role != "radiogroup" && node.role != "dialog" && !node.disabled())
            .filter(|node| self.primary_action(state, &node.id).is_some())
            .collect();
        for (position, node) in actionable.iter().enumerate() {
            let component = state.component(&node.id);
            let hint = component.and_then(|component| component.voice.as_ref());
            let mut score = voice::phrase_score(words, &node.a11y.label);
            for synonym in hint
                .map(|hint| hint.synonyms.as_slice())
                .unwrap_or_default()
            {
                score = score.max(voice::phrase_score(words, synonym));
            }
            if choices.is_empty()
                && let Some(ordinal) = spoken_ordinal
            {
                let by_hint = match (ordinal, hint.and_then(|hint| hint.ordinal)) {
                    (Ordinal::Nth(n), Some(declared)) => usize::try_from(declared).ok() == Some(n),
                    _ => false,
                };
                if by_hint || ordinal.position(actionable.len()) == Some(position) {
                    score = score.max(voice::ORDINAL_MATCH);
                }
            }
            consider(
                score,
                Target::Component {
                    component: node.id.clone(),
                },
            );
        }
        let (_, target) = best?;
        match target {
            Target::Option { component, option } => {
                self.select_option(surface, &component, &option)
            }
            Target::Component { component } => {
                let prop = self.primary_action(state, &component)?;
                let action = self.fire_from(surface, &component, &prop, None, ActionSource::Voice);
                Some(VoiceMatch {
                    component,
                    option: None,
                    batch: RenderBatch::empty(surface),
                    action,
                })
            }
        }
    }

    fn primary_action(&self, state: &SurfaceState, id: &str) -> Option<String> {
        let component = state.component(id)?;
        let definition = self.catalogs.get(&component.type_name)?;
        definition
            .action_props()
            .find(|name| component.props.get(*name).and_then(view::handle).is_some())
            .map(str::to_string)
    }

    fn select_option(
        &mut self,
        surface: &str,
        component: &str,
        option: &str,
    ) -> Option<VoiceMatch> {
        let state = self.surfaces.get(surface)?;
        let node = state.node(component)?;
        let path = node.props.get("value")?.as_str()?.to_string();
        let multiple = node.props.get("multiple") == Some(&Value::Bool(true));
        let current = pointer::get(&state.data, &path).cloned();
        let next = if multiple {
            let mut selected: Vec<Value> = current
                .and_then(|value| value.as_array().cloned())
                .unwrap_or_default();
            match selected
                .iter()
                .position(|value| value.as_str() == Some(option))
            {
                Some(index) => {
                    selected.remove(index);
                }
                None => selected.push(Value::from(option)),
            }
            Value::Array(selected)
        } else {
            Value::from(option)
        };
        let batch = self.write_value(surface, &path, next);
        let has_change = self
            .surfaces
            .get(surface)
            .and_then(|state| state.component(component))
            .and_then(|raw| raw.props.get("on_change"))
            .and_then(view::handle)
            .is_some();
        let action = if has_change {
            self.fire_from(surface, component, "on_change", None, ActionSource::Voice)
        } else {
            None
        };
        Some(VoiceMatch {
            component: component.to_string(),
            option: Some(option.to_string()),
            batch,
            action,
        })
    }
}

enum Target {
    Option { component: String, option: String },
    Component { component: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn node(id: &str, type_name: &str, children: &[&str]) -> Node {
        Node {
            id: id.into(),
            type_name: type_name.into(),
            role: "group".into(),
            props: Map::new(),
            value: None,
            a11y: A11y {
                role: "group".into(),
                label: String::new(),
            },
            children: children.iter().map(|child| child.to_string()).collect(),
            say: String::new(),
            fallback: false,
            reachable: true,
        }
    }

    #[test]
    fn diff_orders_ops_for_renderers() {
        let old = vec![
            node("a", "Card", &["b"]),
            node("b", "Text", &[]),
            node("gone", "Text", &[]),
        ];
        let new = vec![
            node("a", "Card", &["b", "c"]),
            node("b", "Badge", &[]),
            node("c", "Row", &["x"]),
            node("x", "Text", &[]),
        ];
        let ops: Vec<Value> = diff(&old, &new)
            .iter()
            .map(|op| serde_json::to_value(op).unwrap_or(Value::Null))
            .collect();
        let summary: Vec<String> = ops
            .iter()
            .map(|op| {
                format!(
                    "{} {}",
                    op["op"].as_str().unwrap_or(""),
                    op["id"].as_str().unwrap_or("")
                )
            })
            .collect();
        assert_eq!(
            summary,
            vec![
                "remove b",
                "create b",
                "create c",
                "create x",
                "children a",
                "children c",
                "remove gone"
            ]
        );
        assert!(diff(&new, &new).is_empty());
    }

    #[test]
    fn sequence_rules() {
        let hello = json!({ "v": 1, "t": "hello", "modalities": ["screen"] }).to_string();
        let mut client = Client::new(&hello).unwrap_or_else(|error| panic!("{error}"));
        assert!(client.catalogs().catalogs().is_empty());
        client.receive(&json!({ "v": 1, "t": "ui.open", "surface": "s1", "seq": 1, "root": "t", "components": [] }).to_string());
        assert!(matches!(client.sequence("s1", 2), Sequence::Apply));
        assert!(matches!(client.sequence("s1", 1), Sequence::Duplicate));
        assert!(matches!(client.sequence("s1", 5), Sequence::Gap));
        assert!(matches!(client.sequence("s1", 2), Sequence::Awaiting));
        assert!(matches!(client.sequence("s9", 1), Sequence::Unknown));
    }
}
