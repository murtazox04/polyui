use crate::Map;
use crate::Value;
use std::fmt;

pub const CORE_JSON: &str = include_str!("../../../spec/catalogs/core.json");
pub const CHART_JSON: &str = include_str!("../../../spec/catalogs/chart.json");
pub const EMBED_JSON: &str = include_str!("../../../spec/catalogs/embed.json");
pub const EMBED_ID: &str = "embed";

pub const ROLES: [&str; 11] = [
    "text",
    "group",
    "list",
    "listitem",
    "button",
    "radiogroup",
    "img",
    "textbox",
    "dialog",
    "figure",
    "status",
];

const RESERVED_IDS: [&str; 3] = ["core", "chart", EMBED_ID];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogError {
    pub path: String,
    pub message: String,
}

impl CatalogError {
    fn new(path: &str, message: impl Into<String>) -> Self {
        Self {
            path: path.to_string(),
            message: message.into(),
        }
    }
}

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            write!(f, "{}", self.message)
        } else {
            write!(f, "{}: {}", self.path, self.message)
        }
    }
}

impl std::error::Error for CatalogError {}

#[derive(Debug, Clone, PartialEq)]
pub enum PropKind {
    String { min: Option<u64>, max: Option<u64> },
    Int { min: Option<i64>, max: Option<i64> },
    Number { min: Option<f64>, max: Option<f64> },
    Bool,
    Scalar,
    Enum { values: Vec<String> },
    Url { schemes: Vec<String> },
    Action,
    Path,
    Array { of: Box<PropSpec>, max: Option<u64> },
    Object { fields: Vec<(String, PropSpec)> },
}

impl PropKind {
    pub fn name(&self) -> &'static str {
        match self {
            Self::String { .. } => "string",
            Self::Int { .. } => "int",
            Self::Number { .. } => "number",
            Self::Bool => "bool",
            Self::Scalar => "scalar",
            Self::Enum { .. } => "enum",
            Self::Url { .. } => "url",
            Self::Action => "action",
            Self::Path => "path",
            Self::Array { .. } => "array",
            Self::Object { .. } => "object",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PropSpec {
    pub kind: PropKind,
    pub required: bool,
    pub default: Option<Value>,
    pub bindable: bool,
    pub description: Option<String>,
}

impl PropSpec {
    pub fn from_value(value: &Value) -> Result<Self, CatalogError> {
        parse_prop(value, "")
    }

    pub fn field(&self, name: &str) -> Option<&PropSpec> {
        match &self.kind {
            PropKind::Object { fields } => lookup(fields, name),
            _ => None,
        }
    }
}

pub(crate) fn lookup<'a>(entries: &'a [(String, PropSpec)], name: &str) -> Option<&'a PropSpec> {
    entries
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, spec)| spec)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChildRule {
    None,
    Any,
    Only(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeDef {
    pub name: String,
    pub qualified: String,
    pub description: String,
    pub role: String,
    pub children: ChildRule,
    pub props: Vec<(String, PropSpec)>,
    pub alt: String,
    pub say: String,
}

impl TypeDef {
    pub fn prop(&self, name: &str) -> Option<&PropSpec> {
        lookup(&self.props, name)
    }

    pub fn prop_names(&self) -> Vec<&str> {
        self.props.iter().map(|(name, _)| name.as_str()).collect()
    }

    pub fn has_children(&self) -> bool {
        self.children != ChildRule::None
    }

    pub fn allows_child(&self, qualified: &str) -> bool {
        match &self.children {
            ChildRule::None => false,
            ChildRule::Any => true,
            ChildRule::Only(names) => names.iter().any(|name| name == qualified),
        }
    }

    pub fn action_props(&self) -> impl Iterator<Item = &str> {
        self.props
            .iter()
            .filter(|(_, spec)| spec.kind == PropKind::Action)
            .map(|(name, _)| name.as_str())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Catalog {
    pub id: String,
    pub version: u64,
    pub types: Vec<TypeDef>,
}

impl Catalog {
    pub fn from_json(text: &str) -> Result<Self, CatalogError> {
        let value = crate::codec::read(text)
            .map_err(|error| CatalogError::new("", format!("invalid JSON: {error}")))?;
        Self::from_value(&value)
    }

    pub fn from_value(value: &Value) -> Result<Self, CatalogError> {
        let map = as_object(value, "")?;
        reject_unknown(map, "", &["id", "version", "types"])?;
        let id = map
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| CatalogError::new("id", "must be a string"))?;
        if id.is_empty()
            || !id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        {
            return Err(CatalogError::new(
                "id",
                "must be lowercase letters and digits",
            ));
        }
        let version = map
            .get("version")
            .and_then(Value::as_u64)
            .filter(|version| *version >= 1)
            .ok_or_else(|| CatalogError::new("version", "must be a positive integer"))?;
        let types_map = map
            .get("types")
            .and_then(Value::as_object)
            .ok_or_else(|| CatalogError::new("types", "must be an object"))?;
        let mut types = Vec::with_capacity(types_map.len());
        for (name, definition) in types_map {
            types.push(parse_type(id, name, definition)?);
        }
        Ok(Self {
            id: id.to_string(),
            version,
            types,
        })
    }

    pub fn get(&self, name: &str) -> Option<&TypeDef> {
        self.types.iter().find(|definition| definition.name == name)
    }

    pub fn tag(&self) -> String {
        format!("{}@{}", self.id, self.version)
    }
}

fn as_object<'a>(value: &'a Value, path: &str) -> Result<&'a Map<String, Value>, CatalogError> {
    value
        .as_object()
        .ok_or_else(|| CatalogError::new(path, "must be an object"))
}

fn reject_unknown(
    map: &Map<String, Value>,
    path: &str,
    known: &[&str],
) -> Result<(), CatalogError> {
    match map.keys().find(|key| !known.contains(&key.as_str())) {
        Some(key) => Err(CatalogError::new(path, format!("unknown field \"{key}\""))),
        None => Ok(()),
    }
}

fn join_path(base: &str, key: &str) -> String {
    if base.is_empty() {
        key.to_string()
    } else {
        format!("{base}.{key}")
    }
}

pub(crate) fn is_type_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_uppercase()) && chars.all(|c| c.is_ascii_alphanumeric())
}

fn is_qualified_type_name(name: &str) -> bool {
    match name.split_once(':') {
        Some((namespace, local)) => {
            !namespace.is_empty()
                && namespace
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
                && is_type_name(local)
        }
        None => is_type_name(name),
    }
}

fn is_prop_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn parse_type(catalog_id: &str, name: &str, value: &Value) -> Result<TypeDef, CatalogError> {
    let path = format!("types.{name}");
    if !is_type_name(name) {
        return Err(CatalogError::new(&path, "type names must be PascalCase"));
    }
    let map = as_object(value, &path)?;
    reject_unknown(
        map,
        &path,
        &["description", "role", "children", "props", "alt", "say"],
    )?;
    let text_field = |key: &str, required: bool| -> Result<String, CatalogError> {
        match map.get(key) {
            Some(Value::String(text)) => Ok(text.clone()),
            None if !required => Ok(String::new()),
            _ => Err(CatalogError::new(
                &join_path(&path, key),
                "must be a string",
            )),
        }
    };
    let description = text_field("description", true)?;
    let role = text_field("role", true)?;
    if !ROLES.contains(&role.as_str()) {
        return Err(CatalogError::new(
            &join_path(&path, "role"),
            format!("unknown role \"{role}\""),
        ));
    }
    let children = match map.get("children") {
        None | Some(Value::Bool(false)) => ChildRule::None,
        Some(Value::Bool(true)) => ChildRule::Any,
        Some(Value::Array(items)) => {
            let mut names = Vec::with_capacity(items.len());
            for item in items {
                match item.as_str() {
                    Some(child) if is_qualified_type_name(child) => names.push(child.to_string()),
                    _ => {
                        return Err(CatalogError::new(
                            &join_path(&path, "children"),
                            "must list type names",
                        ));
                    }
                }
            }
            ChildRule::Only(names)
        }
        Some(_) => {
            return Err(CatalogError::new(
                &join_path(&path, "children"),
                "must be true, false or a list of type names",
            ));
        }
    };
    let mut props = Vec::new();
    if let Some(value) = map.get("props") {
        let props_path = join_path(&path, "props");
        for (prop_name, spec) in as_object(value, &props_path)? {
            let prop_path = join_path(&props_path, prop_name);
            if !is_prop_name(prop_name) {
                return Err(CatalogError::new(
                    &prop_path,
                    "prop names must be snake_case",
                ));
            }
            props.push((prop_name.clone(), parse_prop(spec, &prop_path)?));
        }
    }
    let qualified = if catalog_id == "core" {
        name.to_string()
    } else {
        format!("{catalog_id}:{name}")
    };
    Ok(TypeDef {
        name: name.to_string(),
        qualified,
        description,
        role,
        children,
        props,
        alt: text_field("alt", false)?,
        say: text_field("say", false)?,
    })
}

fn parse_prop(value: &Value, path: &str) -> Result<PropSpec, CatalogError> {
    let map = as_object(value, path)?;
    let kind_name = map
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| CatalogError::new(path, "missing \"kind\""))?;
    let common = ["kind", "required", "default", "bindable", "description"];
    let allowed: &[&str] = match kind_name {
        "string" | "int" | "number" => &["min", "max"],
        "enum" => &["values"],
        "url" => &["schemes"],
        "array" => &["of", "max"],
        "object" => &["fields"],
        "bool" | "scalar" | "action" | "path" => &[],
        other => {
            return Err(CatalogError::new(path, format!("unknown kind \"{other}\"")));
        }
    };
    if let Some(key) = map
        .keys()
        .find(|key| !common.contains(&key.as_str()) && !allowed.contains(&key.as_str()))
    {
        return Err(CatalogError::new(path, format!("unknown field \"{key}\"")));
    }
    let unsigned = |key: &str| -> Result<Option<u64>, CatalogError> {
        match map.get(key) {
            None => Ok(None),
            Some(value) => value
                .as_u64()
                .map(Some)
                .ok_or_else(|| CatalogError::new(&join_path(path, key), "must be a whole number")),
        }
    };
    let kind = match kind_name {
        "string" => PropKind::String {
            min: unsigned("min")?,
            max: unsigned("max")?,
        },
        "int" => {
            let signed = |key: &str| -> Result<Option<i64>, CatalogError> {
                match map.get(key) {
                    None => Ok(None),
                    Some(value) => value.as_i64().map(Some).ok_or_else(|| {
                        CatalogError::new(&join_path(path, key), "must be a whole number")
                    }),
                }
            };
            PropKind::Int {
                min: signed("min")?,
                max: signed("max")?,
            }
        }
        "number" => {
            let float = |key: &str| -> Result<Option<f64>, CatalogError> {
                match map.get(key) {
                    None => Ok(None),
                    Some(value) => value.as_f64().map(Some).ok_or_else(|| {
                        CatalogError::new(&join_path(path, key), "must be a number")
                    }),
                }
            };
            PropKind::Number {
                min: float("min")?,
                max: float("max")?,
            }
        }
        "bool" => PropKind::Bool,
        "scalar" => PropKind::Scalar,
        "enum" => {
            let values = string_list(map.get("values"), &join_path(path, "values"))?;
            if values.is_empty() {
                return Err(CatalogError::new(
                    &join_path(path, "values"),
                    "must not be empty",
                ));
            }
            PropKind::Enum { values }
        }
        "url" => {
            let schemes = match map.get("schemes") {
                None => vec!["https".to_string()],
                Some(value) => string_list(Some(value), &join_path(path, "schemes"))?
                    .into_iter()
                    .map(|scheme| scheme.to_ascii_lowercase())
                    .collect(),
            };
            if let Some(blocked) = schemes
                .iter()
                .find(|scheme| crate::sanitize::BLOCKED_SCHEMES.contains(&scheme.as_str()))
            {
                return Err(CatalogError::new(
                    &join_path(path, "schemes"),
                    format!("scheme \"{blocked}\" is never allowed"),
                ));
            }
            PropKind::Url { schemes }
        }
        "action" => PropKind::Action,
        "path" => PropKind::Path,
        "array" => {
            let of_path = join_path(path, "of");
            let of = map
                .get("of")
                .ok_or_else(|| CatalogError::new(&of_path, "is required for arrays"))?;
            PropKind::Array {
                of: Box::new(parse_prop(of, &of_path)?),
                max: unsigned("max")?,
            }
        }
        _ => {
            let fields_path = join_path(path, "fields");
            let fields_map = map
                .get("fields")
                .ok_or_else(|| CatalogError::new(&fields_path, "is required for objects"))
                .and_then(|value| as_object(value, &fields_path))?;
            let mut fields = Vec::with_capacity(fields_map.len());
            for (field_name, spec) in fields_map {
                let field_path = join_path(&fields_path, field_name);
                if !is_prop_name(field_name) {
                    return Err(CatalogError::new(
                        &field_path,
                        "field names must be snake_case",
                    ));
                }
                fields.push((field_name.clone(), parse_prop(spec, &field_path)?));
            }
            PropKind::Object { fields }
        }
    };
    let flag = |key: &str| -> Result<Option<bool>, CatalogError> {
        match map.get(key) {
            None => Ok(None),
            Some(Value::Bool(flag)) => Ok(Some(*flag)),
            Some(_) => Err(CatalogError::new(
                &join_path(path, key),
                "must be true or false",
            )),
        }
    };
    let never_bindable = matches!(kind, PropKind::Action | PropKind::Path);
    let bindable = match flag("bindable")? {
        Some(true) if never_bindable => {
            return Err(CatalogError::new(
                &join_path(path, "bindable"),
                format!("{kind_name} props can never be bound"),
            ));
        }
        Some(flag) => flag,
        None => !never_bindable,
    };
    let description = match map.get("description") {
        None => None,
        Some(Value::String(text)) => Some(text.clone()),
        Some(_) => {
            return Err(CatalogError::new(
                &join_path(path, "description"),
                "must be a string",
            ));
        }
    };
    let spec = PropSpec {
        kind,
        required: flag("required")?.unwrap_or(false),
        default: map.get("default").cloned(),
        bindable,
        description,
    };
    if let Some(default) = &spec.default
        && let Err(message) = default_fits(&spec, default)
    {
        return Err(CatalogError::new(
            &join_path(path, "default"),
            format!("default does not fit the prop: {message}"),
        ));
    }
    Ok(spec)
}

#[cfg(not(feature = "client-lite"))]
fn default_fits(spec: &PropSpec, default: &Value) -> Result<(), String> {
    crate::validate::check_literal(spec, default)
}

#[cfg(feature = "client-lite")]
fn default_fits(spec: &PropSpec, default: &Value) -> Result<(), String> {
    if crate::fits::fits(spec, default, true) {
        Ok(())
    } else {
        Err("the value is not allowed for this kind".to_string())
    }
}

fn string_list(value: Option<&Value>, path: &str) -> Result<Vec<String>, CatalogError> {
    let items = value
        .and_then(Value::as_array)
        .ok_or_else(|| CatalogError::new(path, "must be a list of strings"))?;
    let mut out: Vec<String> = Vec::with_capacity(items.len());
    for item in items {
        let text = item
            .as_str()
            .ok_or_else(|| CatalogError::new(path, "must be a list of strings"))?;
        if out.iter().any(|existing| existing == text) {
            return Err(CatalogError::new(
                path,
                format!("\"{text}\" is listed twice"),
            ));
        }
        out.push(text.to_string());
    }
    Ok(out)
}

#[cfg(any(not(feature = "client-lite"), feature = "catalog-checks"))]
fn extension(value: &Value) -> Result<Catalog, CatalogError> {
    Catalog::from_value(value)
}

#[cfg(all(feature = "client-lite", not(feature = "catalog-checks")))]
fn extension(value: &Value) -> Result<Catalog, CatalogError> {
    builtin::extension(value)
}

mod builtin {
    #[cfg(all(feature = "client-lite", not(feature = "catalog-checks")))]
    use super::CatalogError;
    use super::{Catalog, ChildRule, PropKind, PropSpec, TypeDef};
    use crate::{Map, Value};

    fn text(map: &Map<String, Value>, key: &str) -> String {
        map.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    }

    fn strings(value: Option<&Value>) -> Vec<String> {
        value
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn entries(value: Option<&Value>) -> Vec<(String, PropSpec)> {
        value
            .and_then(Value::as_object)
            .map(|map| {
                map.iter()
                    .filter_map(|(name, spec)| prop(spec).map(|spec| (name.clone(), spec)))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn prop(value: &Value) -> Option<PropSpec> {
        let map = value.as_object()?;
        let unsigned = |key: &str| map.get(key).and_then(Value::as_u64);
        let kind = match map.get("kind")?.as_str()? {
            "string" => PropKind::String {
                min: unsigned("min"),
                max: unsigned("max"),
            },
            "int" => PropKind::Int {
                min: map.get("min").and_then(Value::as_i64),
                max: map.get("max").and_then(Value::as_i64),
            },
            "number" => PropKind::Number {
                min: map.get("min").and_then(Value::as_f64),
                max: map.get("max").and_then(Value::as_f64),
            },
            "bool" => PropKind::Bool,
            "scalar" => PropKind::Scalar,
            "enum" => PropKind::Enum {
                values: strings(map.get("values")),
            },
            "url" => PropKind::Url {
                schemes: match map.get("schemes") {
                    None => vec!["https".to_string()],
                    some => strings(some)
                        .iter()
                        .map(|scheme| scheme.to_ascii_lowercase())
                        .collect(),
                },
            },
            "action" => PropKind::Action,
            "path" => PropKind::Path,
            "array" => PropKind::Array {
                of: Box::new(prop(map.get("of")?)?),
                max: unsigned("max"),
            },
            "object" => PropKind::Object {
                fields: entries(map.get("fields")),
            },
            _ => return None,
        };
        let flag = |key: &str| map.get(key).and_then(Value::as_bool);
        let bindable =
            flag("bindable").unwrap_or(!matches!(kind, PropKind::Action | PropKind::Path));
        Some(PropSpec {
            kind,
            required: flag("required").unwrap_or(false),
            default: map.get("default").cloned(),
            bindable,
            description: map
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
    }

    pub(super) fn catalog(json: &str) -> Option<Catalog> {
        from_value(&crate::codec::read(json).ok()?)
    }

    #[cfg(all(feature = "client-lite", not(feature = "catalog-checks")))]
    pub(super) fn extension(value: &Value) -> Result<Catalog, CatalogError> {
        let catalog = from_value(value)
            .ok_or_else(|| CatalogError::new("", "needs a string id, a version and types"))?;
        let declared = value.get("types").and_then(Value::as_object);
        for (name, definition) in declared.into_iter().flatten() {
            let props = definition
                .get("props")
                .and_then(Value::as_object)
                .map_or(0, Map::len);
            let read = catalog
                .get(name)
                .ok_or_else(|| CatalogError::new(&format!("types.{name}"), "must be an object"))?;
            if read.props.len() != props {
                return Err(CatalogError::new(
                    &format!("types.{name}.props"),
                    "has a prop with an unknown kind",
                ));
            }
        }
        Ok(catalog)
    }

    fn from_value(value: &Value) -> Option<Catalog> {
        let id = value.get("id")?.as_str()?;
        let types = value
            .get("types")?
            .as_object()?
            .iter()
            .filter_map(|(name, definition)| {
                let map = definition.as_object()?;
                let children = match map.get("children") {
                    Some(Value::Bool(true)) => ChildRule::Any,
                    Some(Value::Array(_)) => ChildRule::Only(strings(map.get("children"))),
                    _ => ChildRule::None,
                };
                Some(TypeDef {
                    name: name.clone(),
                    qualified: if id == "core" {
                        name.clone()
                    } else {
                        format!("{id}:{name}")
                    },
                    description: text(map, "description"),
                    role: text(map, "role"),
                    children,
                    props: entries(map.get("props")),
                    alt: text(map, "alt"),
                    say: text(map, "say"),
                })
            })
            .collect();
        Some(Catalog {
            id: id.to_string(),
            version: value.get("version")?.as_u64()?,
            types,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CatalogSet {
    catalogs: Vec<Catalog>,
}

impl CatalogSet {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn builtin() -> Self {
        Self {
            catalogs: [CORE_JSON, CHART_JSON]
                .into_iter()
                .filter_map(builtin::catalog)
                .collect(),
        }
    }

    pub fn with_embed(mut self) -> Self {
        if !self.catalogs.iter().any(|catalog| catalog.id == EMBED_ID)
            && let Some(embed) = builtin::catalog(EMBED_JSON)
        {
            self.catalogs.push(embed);
        }
        self
    }

    pub fn with(mut self, catalog: Catalog) -> Result<Self, CatalogError> {
        if RESERVED_IDS.contains(&catalog.id.as_str()) {
            return Err(CatalogError::new(
                "id",
                format!("\"{}\" is reserved", catalog.id),
            ));
        }
        if self
            .catalogs
            .iter()
            .any(|existing| existing.id == catalog.id)
        {
            return Err(CatalogError::new(
                "id",
                format!("catalog \"{}\" is already in the set", catalog.id),
            ));
        }
        self.catalogs.push(catalog);
        Ok(self)
    }

    pub fn with_extensions_json(self, text: &str) -> Result<Self, CatalogError> {
        let value = crate::codec::read(text)
            .map_err(|error| CatalogError::new("", format!("invalid JSON: {error}")))?;
        let items = value
            .as_array()
            .ok_or_else(|| CatalogError::new("", "must be a list of catalogs"))?;
        items
            .iter()
            .enumerate()
            .try_fold(self, |set, (index, item)| {
                extension(item)
                    .and_then(|catalog| set.with(catalog))
                    .map_err(|error| {
                        let path = if error.path.is_empty() {
                            format!("[{index}]")
                        } else {
                            format!("[{index}].{}", error.path)
                        };
                        CatalogError::new(&path, error.message)
                    })
            })
    }

    pub fn catalogs(&self) -> &[Catalog] {
        &self.catalogs
    }

    pub fn get(&self, type_name: &str) -> Option<&TypeDef> {
        let (namespace, local) = type_name.split_once(':').unwrap_or(("core", type_name));
        if namespace == "core" && type_name.contains(':') {
            return None;
        }
        self.catalogs
            .iter()
            .find(|catalog| catalog.id == namespace)
            .and_then(|catalog| catalog.get(local))
    }

    pub fn type_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self
            .catalogs
            .iter()
            .flat_map(|catalog| {
                catalog
                    .types
                    .iter()
                    .map(|definition| definition.qualified.as_str())
            })
            .collect();
        names.sort_unstable();
        names
    }

    pub fn types(&self) -> impl Iterator<Item = &TypeDef> {
        self.catalogs
            .iter()
            .flat_map(|catalog| catalog.types.iter())
    }

    pub fn restrict(&self, declared: &[String]) -> Self {
        Self {
            catalogs: self
                .catalogs
                .iter()
                .filter(|catalog| declares(declared, catalog))
                .cloned()
                .collect(),
        }
    }

    pub(crate) fn keep_declared(&mut self, declared: &[String]) {
        self.catalogs.retain(|catalog| declares(declared, catalog));
    }

    pub fn negotiate(&self, declared: &[String]) -> Vec<String> {
        self.catalogs
            .iter()
            .filter(|catalog| declares(declared, catalog))
            .map(Catalog::tag)
            .collect()
    }
}

fn declares(declared: &[String], catalog: &Catalog) -> bool {
    declared.iter().any(|entry| {
        entry.split_once('@').is_some_and(|(id, major)| {
            id == catalog.id && major.parse::<u64>().ok() == Some(catalog.version)
        })
    })
}

#[cfg(all(test, not(feature = "client-lite")))]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn builtin_catalogs_load() {
        let set = CatalogSet::builtin();
        assert_eq!(set.catalogs().len(), 2);
        assert!(set.get("Button").is_some());
        assert!(set.get("chart:Chart").is_some());
        assert!(set.get("Chart").is_none());
        assert!(set.get("core:Button").is_none());
        assert_eq!(
            set.get("chart:Dashboard").map(|t| t.role.as_str()),
            Some("group")
        );
        assert!(Catalog::from_json(CORE_JSON).is_ok());
        assert!(Catalog::from_json(CHART_JSON).is_ok());
    }

    #[test]
    fn builtin_shortcut_matches_the_checked_parser() {
        for json in [CORE_JSON, CHART_JSON, EMBED_JSON] {
            assert_eq!(builtin::catalog(json), Catalog::from_json(json).ok());
        }
    }

    #[test]
    fn embed_is_offered_only_when_the_host_turns_it_on() {
        let default = CatalogSet::builtin();
        assert!(default.get("embed:Embed").is_none());
        assert_eq!(
            default.negotiate(&["core@1".into(), "chart@1".into(), "embed@1".into()]),
            vec!["core@1", "chart@1"]
        );
        let enabled = CatalogSet::builtin().with_embed().with_embed();
        assert_eq!(enabled.catalogs().len(), 3);
        let embed = enabled.get("embed:Embed").unwrap();
        assert_eq!(embed.role, "figure");
        assert!(!embed.prop("resource").unwrap().bindable);
        assert_eq!(
            embed.prop("resource").unwrap().kind,
            PropKind::Url {
                schemes: vec!["ui".to_string()]
            }
        );
        assert_eq!(embed.action_props().collect::<Vec<_>>(), vec!["on_message"]);
        assert_eq!(
            enabled.negotiate(&["embed@1".into()]),
            vec!["embed@1".to_string()]
        );
        let fake =
            Catalog::from_value(&json!({ "id": "embed", "version": 1, "types": {} })).unwrap();
        assert!(CatalogSet::builtin().with(fake).is_err());
    }

    #[test]
    fn restriction_uses_major_versions() {
        let set = CatalogSet::builtin();
        let core_only = set.restrict(&["core@1".to_string(), "chart@2".to_string()]);
        assert!(core_only.get("Button").is_some());
        assert!(core_only.get("chart:Chart").is_none());
        assert_eq!(
            set.negotiate(&["chart@1".into(), "core@1".into(), "acme@1".into()]),
            vec!["core@1", "chart@1"]
        );
        assert!(
            set.restrict(&["core".into(), "@1".into()])
                .catalogs()
                .is_empty()
        );
    }

    #[test]
    fn extensions_are_namespaced() {
        let acme = Catalog::from_value(&json!({
            "id": "acme",
            "version": 1,
            "types": { "Card": { "description": "x", "role": "group", "children": ["Text", "acme:Card"], "props": {} } }
        }))
        .unwrap();
        let set = CatalogSet::builtin().with(acme).unwrap();
        let card = set.get("acme:Card").unwrap();
        assert_eq!(card.qualified, "acme:Card");
        assert!(card.allows_child("Text"));
        assert!(!card.allows_child("Button"));
        assert!(set.get("Card").is_some());
        let again =
            Catalog::from_value(&json!({ "id": "acme", "version": 1, "types": {} })).unwrap();
        assert!(set.clone().with(again).is_err());
        let reserved =
            Catalog::from_value(&json!({ "id": "core", "version": 1, "types": {} })).unwrap();
        assert!(set.with(reserved).is_err());
    }

    #[test]
    fn extension_lists_are_read_and_checked() {
        let acme = r#"[{ "id": "acme", "version": 1, "types": { "Badge2": { "description": "x", "role": "status", "props": { "text": { "kind": "string", "required": true } } } } }]"#;
        let set = CatalogSet::builtin().with_extensions_json(acme).unwrap();
        assert_eq!(set.get("acme:Badge2").unwrap().role, "status");
        let failures = [
            ("{", ""),
            ("{}", ""),
            (
                r#"[{ "id": "acme", "version": 1, "types": { "badge": {} } }]"#,
                "[0].types.badge",
            ),
            (r#"[{ "id": "core", "version": 1, "types": {} }]"#, "[0].id"),
            (
                r#"[{ "id": "acme", "version": 1, "types": {} }, { "id": "acme", "version": 1, "types": {} }]"#,
                "[1].id",
            ),
        ];
        for (input, path) in failures {
            let error = CatalogSet::builtin()
                .with_extensions_json(input)
                .unwrap_err();
            assert_eq!(error.path, path, "{input}: {error}");
        }
    }

    #[test]
    fn bad_catalogs_are_rejected() {
        let cases = [
            json!({ "id": "Acme", "version": 1, "types": {} }),
            json!({ "id": "acme", "version": 0, "types": {} }),
            json!({ "id": "acme", "version": 1, "types": { "card": { "description": "", "role": "group" } } }),
            json!({ "id": "acme", "version": 1, "types": { "Card": { "description": "", "role": "box" } } }),
            json!({ "id": "acme", "version": 1, "types": { "Card": { "description": "", "role": "group", "props": { "x": { "kind": "strng" } } } } }),
            json!({ "id": "acme", "version": 1, "types": { "Card": { "description": "", "role": "group", "props": { "x": { "kind": "action", "bindable": true } } } } }),
            json!({ "id": "acme", "version": 1, "types": { "Card": { "description": "", "role": "group", "props": { "x": { "kind": "url", "schemes": ["javascript"] } } } } }),
            json!({ "id": "acme", "version": 1, "types": { "Card": { "description": "", "role": "group", "props": { "x": { "kind": "enum", "values": ["a"], "default": "b" } } } } }),
            json!({ "id": "acme", "version": 1, "types": { "Card": { "description": "", "role": "group", "props": { "Bad": { "kind": "bool" } } } } }),
            json!({ "id": "acme", "version": 1, "types": { "Card": { "description": "", "role": "group", "props": { "x": { "kind": "array" } } } } }),
        ];
        for case in cases {
            assert!(Catalog::from_value(&case).is_err(), "accepted {case}");
        }
    }

    #[test]
    fn defaults_and_bindable_flags() {
        let set = CatalogSet::builtin();
        let button = set.get("Button").unwrap();
        assert_eq!(
            button.prop("tone").unwrap().default,
            Some(json!("secondary"))
        );
        assert!(button.prop("label").unwrap().bindable);
        assert!(!button.prop("on_press").unwrap().bindable);
        assert_eq!(button.action_props().collect::<Vec<_>>(), vec!["on_press"]);
        let rows = set.get("chart:Chart").unwrap().prop("rows").unwrap();
        let PropKind::Array { of, .. } = &rows.kind else {
            panic!("rows must be an array");
        };
        assert_eq!(of.field("x").map(|spec| spec.kind.name()), Some("scalar"));
    }
}
