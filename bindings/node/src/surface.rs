use crate::host::{
    Prepared as Outcome, sealed_value, surface_from_value, surface_value, update_value,
};
use napi::{Error, Result, Status};
use napi_derive::napi;
use polyui_core::SealedAction;
use polyui_core::validate::{Surface as CoreSurface, Update as CoreUpdate};
use serde_json::Value;

#[napi]
#[derive(Clone)]
pub struct Surface {
    pub(crate) inner: CoreSurface,
}

impl From<CoreSurface> for Surface {
    fn from(inner: CoreSurface) -> Self {
        Self { inner }
    }
}

fn to_value<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

#[napi]
impl Surface {
    #[napi(factory, js_name = "fromJSON")]
    pub fn from_json(value: Value) -> Result<Self> {
        surface_from_value(value)
            .map(Self::from)
            .map_err(|message| Error::new(Status::InvalidArg, message))
    }

    #[napi(js_name = "toJSON")]
    pub fn to_json(&self) -> Value {
        surface_value(&self.inner)
    }

    #[napi(getter)]
    pub fn id(&self) -> Option<String> {
        self.inner.id.clone()
    }

    #[napi(getter)]
    pub fn root(&self) -> String {
        self.inner.root.clone()
    }

    #[napi(getter)]
    pub fn components(&self) -> Value {
        to_value(&self.inner.components)
    }

    #[napi(getter)]
    pub fn data(&self) -> Value {
        Value::Object(self.inner.data.clone())
    }

    #[napi]
    pub fn get(&self, id: String) -> Option<Value> {
        self.inner.get(&id).map(to_value)
    }

    #[napi]
    pub fn apply(&self, update: &Update) -> Result<Surface> {
        let mut next = self.inner.clone();
        next.apply(&update.inner)
            .map_err(|error| Error::new(Status::InvalidArg, error.to_string()))?;
        Ok(next.into())
    }

    #[napi]
    pub fn equals(&self, other: &Surface) -> bool {
        self.inner == other.inner
    }
}

#[napi]
#[derive(Clone)]
pub struct Update {
    pub(crate) inner: CoreUpdate,
}

#[napi]
impl Update {
    #[napi(js_name = "toJSON")]
    pub fn to_json(&self) -> Value {
        update_value(&self.inner)
    }

    #[napi(getter)]
    pub fn surface(&self) -> String {
        self.inner.surface.clone()
    }

    #[napi(getter)]
    pub fn upsert(&self) -> Value {
        to_value(&self.inner.upsert)
    }

    #[napi(getter)]
    pub fn remove(&self) -> Vec<String> {
        self.inner.remove.clone()
    }

    #[napi(getter)]
    pub fn root(&self) -> Option<String> {
        self.inner.root.clone()
    }

    #[napi(getter)]
    pub fn data(&self) -> Value {
        to_value(&self.inner.data)
    }
}

pub fn sealed_list(actions: &[SealedAction]) -> Value {
    Value::Array(actions.iter().map(sealed_value).collect())
}

#[napi]
pub struct Prepared {
    pub(crate) inner: Outcome,
}

#[napi]
impl Prepared {
    #[napi(getter)]
    pub fn surface_id(&self) -> String {
        self.inner.surface_id.clone()
    }

    #[napi(getter)]
    pub fn model(&self) -> Surface {
        self.inner.model.clone().into()
    }

    #[napi(getter)]
    pub fn client(&self) -> Surface {
        self.inner.client.clone().into()
    }

    #[napi(getter)]
    pub fn messages(&self) -> Vec<Value> {
        self.inner.messages.clone()
    }

    #[napi(getter)]
    pub fn summary(&self) -> String {
        self.inner.summary.clone()
    }

    #[napi(getter)]
    pub fn actions(&self) -> Value {
        sealed_list(&self.inner.actions)
    }

    #[napi(getter)]
    pub fn seq(&self) -> u32 {
        u32::try_from(self.inner.seq).unwrap_or(u32::MAX)
    }

    #[napi(getter)]
    pub fn touched(&self) -> Vec<String> {
        self.inner.touched.clone()
    }
}
