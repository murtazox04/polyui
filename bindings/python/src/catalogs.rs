use crate::convert::to_json;
use crate::errors::catalog_error;
use polyui_core::{Catalog, CatalogSet};
use pyo3::prelude::*;
use pyo3::types::PyString;

#[pyclass(frozen, eq, name = "Catalogs", module = "polyui")]
#[derive(Clone, PartialEq)]
pub struct PyCatalogs {
    pub inner: CatalogSet,
}

#[pymethods]
impl PyCatalogs {
    #[staticmethod]
    fn builtin() -> Self {
        Self {
            inner: CatalogSet::builtin(),
        }
    }

    #[staticmethod]
    fn empty() -> Self {
        Self {
            inner: CatalogSet::empty(),
        }
    }

    fn with_embed(&self) -> Self {
        Self {
            inner: self.inner.clone().with_embed(),
        }
    }

    fn with_catalog(&self, py: Python<'_>, catalog: &Bound<'_, PyAny>) -> PyResult<Self> {
        let parsed = match catalog.cast::<PyString>() {
            Ok(text) => Catalog::from_json(text.to_str()?),
            Err(_) => Catalog::from_value(&to_json(catalog)?),
        }
        .map_err(|failure| catalog_error(py, &failure))?;
        self.inner
            .clone()
            .with(parsed)
            .map(|inner| Self { inner })
            .map_err(|failure| catalog_error(py, &failure))
    }

    fn restrict(&self, declared: Vec<String>) -> Self {
        Self {
            inner: self.inner.restrict(&declared),
        }
    }

    fn negotiate(&self, declared: Vec<String>) -> Vec<String> {
        self.inner.negotiate(&declared)
    }

    #[getter]
    fn tags(&self) -> Vec<String> {
        self.inner
            .catalogs()
            .iter()
            .map(|catalog| catalog.tag())
            .collect()
    }

    fn type_names(&self) -> Vec<String> {
        self.inner
            .type_names()
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    fn __contains__(&self, type_name: &str) -> bool {
        self.inner.get(type_name).is_some()
    }

    fn __len__(&self) -> usize {
        self.inner.catalogs().len()
    }

    fn __repr__(&self) -> String {
        format!("Catalogs({:?})", self.tags())
    }
}
