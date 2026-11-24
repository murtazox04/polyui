use crate::convert::{to_json, to_py};
use crate::host::{Prepared, sealed_value, surface_from_value, surface_value, update_value};
use polyui_core::validate::{Surface, Update};
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyList;

#[pyclass(frozen, eq, name = "Surface", module = "polyui")]
#[derive(Clone, PartialEq)]
pub struct PySurface {
    pub inner: Surface,
}

impl From<Surface> for PySurface {
    fn from(inner: Surface) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PySurface {
    #[staticmethod]
    fn from_dict(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        surface_from_value(to_json(value)?)
            .map(Self::from)
            .map_err(PyTypeError::new_err)
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        to_py(py, &surface_value(&self.inner))
    }

    #[getter]
    fn id(&self) -> Option<&str> {
        self.inner.id.as_deref()
    }

    #[getter]
    fn root(&self) -> &str {
        &self.inner.root
    }

    #[getter]
    fn components<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        to_py(py, &self.inner.components)
    }

    #[getter]
    fn data<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        to_py(py, &self.inner.data)
    }

    fn get<'py>(&self, py: Python<'py>, id: &str) -> PyResult<Option<Bound<'py, PyAny>>> {
        self.inner
            .get(id)
            .map(|component| to_py(py, component))
            .transpose()
    }

    fn apply(&self, update: &PyUpdate) -> PyResult<Self> {
        let mut next = self.inner.clone();
        next.apply(&update.inner)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        Ok(next.into())
    }

    fn __len__(&self) -> usize {
        self.inner.components.len()
    }

    fn __repr__(&self) -> String {
        format!(
            "Surface(id={}, root={:?}, components={})",
            self.inner
                .id
                .as_deref()
                .map_or_else(|| "None".to_string(), |id| format!("{id:?}")),
            self.inner.root,
            self.inner.components.len()
        )
    }
}

#[pyclass(frozen, eq, name = "Update", module = "polyui")]
#[derive(Clone, PartialEq)]
pub struct PyUpdate {
    pub inner: Update,
}

#[pymethods]
impl PyUpdate {
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        to_py(py, &update_value(&self.inner))
    }

    #[getter]
    fn surface(&self) -> &str {
        &self.inner.surface
    }

    #[getter]
    fn upsert<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        to_py(py, &self.inner.upsert)
    }

    #[getter]
    fn remove(&self) -> Vec<String> {
        self.inner.remove.clone()
    }

    #[getter]
    fn root(&self) -> Option<&str> {
        self.inner.root.as_deref()
    }

    #[getter]
    fn data<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        to_py(py, &self.inner.data)
    }

    fn __repr__(&self) -> String {
        format!(
            "Update(surface={:?}, upsert={}, remove={}, data={})",
            self.inner.surface,
            self.inner.upsert.len(),
            self.inner.remove.len(),
            self.inner.data.len()
        )
    }
}

#[pyclass(frozen, name = "Prepared", module = "polyui")]
pub struct PyPrepared {
    pub inner: Prepared,
}

#[pymethods]
impl PyPrepared {
    #[getter]
    fn surface_id(&self) -> &str {
        &self.inner.surface_id
    }

    #[getter]
    fn model(&self) -> PySurface {
        self.inner.model.clone().into()
    }

    #[getter]
    fn client(&self) -> PySurface {
        self.inner.client.clone().into()
    }

    #[getter]
    fn messages<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        to_py(py, &self.inner.messages)
    }

    #[getter]
    fn summary(&self) -> &str {
        &self.inner.summary
    }

    #[getter]
    fn actions<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        sealed_list(py, &self.inner.actions)
    }

    #[getter]
    fn seq(&self) -> u64 {
        self.inner.seq
    }

    #[getter]
    fn touched(&self) -> Vec<String> {
        self.inner.touched.clone()
    }

    fn __repr__(&self) -> String {
        format!(
            "Prepared(surface_id={:?}, seq={}, messages={}, actions={})",
            self.inner.surface_id,
            self.inner.seq,
            self.inner.messages.len(),
            self.inner.actions.len()
        )
    }
}

pub fn sealed_list<'py>(
    py: Python<'py>,
    actions: &[polyui_core::SealedAction],
) -> PyResult<Bound<'py, PyList>> {
    let items = actions
        .iter()
        .map(|action| to_py(py, &sealed_value(action)))
        .collect::<PyResult<Vec<_>>>()?;
    PyList::new(py, items)
}
