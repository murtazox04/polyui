use crate::host::{operation_name, problems_value};
use napi::bindgen_prelude::{FnArgs, Function, FunctionRef, JsObjectValue, Unknown};
use napi::{Env, Error, JsValue, Result, Status, ValueType};
use polyui_core::CatalogError;
use polyui_core::validate::Problems;
use serde_json::{Value, json};

type FactoryArgs = FnArgs<(String, String, Value)>;
type Factory = FunctionRef<FactoryArgs, Unknown<'static>>;

pub const REF_REFUSED_CODE: &str = "POLYUI_REF_REFUSED";

#[derive(Clone, Copy)]
pub enum Kind {
    Validation,
    Catalog,
    Chart,
    Protocol,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Validation => "ValidationError",
            Self::Catalog => "CatalogError",
            Self::Chart => "ChartError",
            Self::Protocol => "ProtocolError",
        }
    }
}

#[napi_derive::napi]
pub fn use_error_factory(env: Env, factory: Factory) -> Result<()> {
    env.set_instance_data(factory, (), |_| {})
}

pub fn polyui_error(env: &Env, kind: Kind, message: String, details: Value) -> Error {
    let made = env
        .get_instance_data::<Factory>()
        .ok()
        .flatten()
        .and_then(|factory| {
            let create: Function<'_, FactoryArgs, Unknown<'_>> = factory.borrow_back(env).ok()?;
            create
                .call(FnArgs::from((
                    kind.name().to_string(),
                    message.clone(),
                    details,
                )))
                .ok()
        });
    match made {
        Some(instance) => Error::from(instance),
        None => Error::new(Status::GenericFailure, message),
    }
}

pub fn validation_error(env: &Env, problems: &Problems) -> Error {
    polyui_error(
        env,
        Kind::Validation,
        problems.to_string(),
        json!({
            "problems": problems_value(problems),
            "operation": operation_name(problems.operation),
        }),
    )
}

pub fn catalog_error(env: &Env, failure: &CatalogError) -> Error {
    polyui_error(
        env,
        Kind::Catalog,
        failure.to_string(),
        json!({ "path": failure.path }),
    )
}

pub fn chart_error(env: &Env, message: String) -> Error {
    polyui_error(env, Kind::Chart, message, json!({}))
}

pub fn protocol_error(env: &Env, message: String) -> Error {
    polyui_error(env, Kind::Protocol, message, json!({}))
}

fn property_type(value: &Unknown<'_>, name: &str) -> Option<ValueType> {
    if !matches!(value.get_type(), Ok(ValueType::Object)) {
        return None;
    }
    value
        .coerce_to_object()
        .and_then(|object| object.get_named_property::<Unknown<'_>>(name))
        .and_then(|property| property.get_type())
        .ok()
}

pub fn is_refusal(thrown: &Unknown<'_>) -> bool {
    property_type(thrown, "code") == Some(ValueType::String)
        && thrown
            .coerce_to_object()
            .and_then(|object| object.get_named_property::<String>("code"))
            .is_ok_and(|code| code == REF_REFUSED_CODE)
}

pub fn is_thenable(value: &Unknown<'_>) -> bool {
    property_type(value, "then") == Some(ValueType::Function)
}
