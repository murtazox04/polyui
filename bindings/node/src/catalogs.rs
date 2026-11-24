use crate::errors::catalog_error;
use napi::{Env, Result};
use napi_derive::napi;
use polyui_core::{Catalog, CatalogSet};
use serde_json::Value;

#[napi]
#[derive(Clone)]
pub struct Catalogs {
    pub(crate) inner: CatalogSet,
}

#[napi]
impl Catalogs {
    #[napi(factory)]
    pub fn builtin() -> Self {
        Self {
            inner: CatalogSet::builtin(),
        }
    }

    #[napi(factory)]
    pub fn empty() -> Self {
        Self {
            inner: CatalogSet::empty(),
        }
    }

    #[napi]
    pub fn with_catalog(&self, env: Env, catalog: Value) -> Result<Catalogs> {
        let parsed = match &catalog {
            Value::String(text) => Catalog::from_json(text),
            other => Catalog::from_value(other),
        }
        .map_err(|failure| catalog_error(&env, &failure))?;
        self.inner
            .clone()
            .with(parsed)
            .map(|inner| Self { inner })
            .map_err(|failure| catalog_error(&env, &failure))
    }

    #[napi]
    pub fn restrict(&self, declared: Vec<String>) -> Catalogs {
        Self {
            inner: self.inner.restrict(&declared),
        }
    }

    #[napi]
    pub fn negotiate(&self, declared: Vec<String>) -> Vec<String> {
        self.inner.negotiate(&declared)
    }

    #[napi(getter)]
    pub fn tags(&self) -> Vec<String> {
        self.inner
            .catalogs()
            .iter()
            .map(|catalog| catalog.tag())
            .collect()
    }

    #[napi]
    pub fn type_names(&self) -> Vec<String> {
        self.inner
            .type_names()
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    #[napi]
    pub fn has(&self, type_name: String) -> bool {
        self.inner.get(&type_name).is_some()
    }
}
