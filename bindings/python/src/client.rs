use crate::catalogs::PyCatalogs;
use crate::convert::{json_text, to_json, to_py};
use crate::errors::ProtocolError;
use polyui_core::Client;
use pyo3::prelude::*;
use serde_json::Value;

#[pyclass(name = "Client", module = "polyui")]
pub struct PyClient {
    inner: Client,
}

fn optional<'py>(py: Python<'py>, value: Option<Value>) -> PyResult<Option<Bound<'py, PyAny>>> {
    value
        .filter(|found| !found.is_null())
        .map(|found| to_py(py, &found))
        .transpose()
}

#[pymethods]
impl PyClient {
    #[new]
    #[pyo3(signature = (hello, catalogs=None, allow_http=false))]
    fn new(
        hello: &Bound<'_, PyAny>,
        catalogs: Option<&PyCatalogs>,
        allow_http: bool,
    ) -> PyResult<Self> {
        let text = json_text(hello)?;
        let made = match catalogs {
            Some(catalogs) => Client::with_catalogs(&text, catalogs.inner.clone()),
            None => Client::new(&text),
        };
        made.map(|client| Self {
            inner: client.with_allow_http(allow_http),
        })
        .map_err(|error| {
            ProtocolError::new_err(format!(
                "PolyUI could not start a client from this hello: {error}"
            ))
        })
    }

    #[getter]
    fn locale(&self) -> &'static str {
        self.inner.locale().tag()
    }

    #[getter]
    fn session(&self) -> Option<&str> {
        self.inner.session()
    }

    #[getter]
    fn surfaces(&self) -> Vec<String> {
        self.inner
            .surfaces()
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    fn data<'py>(&self, py: Python<'py>, surface: &str) -> PyResult<Option<Bound<'py, PyAny>>> {
        self.inner
            .data(surface)
            .map(|value| to_py(py, value))
            .transpose()
    }

    fn fallback(&self, surface: &str) -> Option<&str> {
        self.inner.fallback(surface)
    }

    fn receive<'py>(
        &mut self,
        py: Python<'py>,
        message: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let text = json_text(message)?;
        to_py(py, &self.inner.receive(&text))
    }

    fn set_value<'py>(
        &mut self,
        py: Python<'py>,
        surface: &str,
        path: &str,
        value: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let text = to_json(value)?.to_string();
        to_py(py, &self.inner.set_value(surface, path, &text))
    }

    #[pyo3(signature = (surface, component, prop, detail=None))]
    fn fire<'py>(
        &self,
        py: Python<'py>,
        surface: &str,
        component: &str,
        prop: &str,
        detail: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Option<Bound<'py, PyAny>>> {
        let detail = detail
            .map(to_json)
            .transpose()?
            .map(|value| value.to_string());
        optional(
            py,
            self.inner.fire(surface, component, prop, detail.as_deref()),
        )
    }

    fn confirm<'py>(
        &mut self,
        py: Python<'py>,
        surface: &str,
        action_id: &str,
        nonce: &str,
    ) -> PyResult<Option<Bound<'py, PyAny>>> {
        optional(py, self.inner.confirm(surface, action_id, nonce))
    }

    fn say<'py>(&self, py: Python<'py>, surface: &str) -> PyResult<Bound<'py, PyAny>> {
        to_py(py, &self.inner.say(surface))
    }

    fn match_voice<'py>(
        &mut self,
        py: Python<'py>,
        surface: &str,
        text: &str,
    ) -> PyResult<Option<Bound<'py, PyAny>>> {
        self.inner
            .match_voice(surface, text)
            .map(|found| to_py(py, &found))
            .transpose()
    }

    fn __repr__(&self) -> String {
        format!(
            "Client(locale={:?}, surfaces={:?})",
            self.inner.locale().tag(),
            self.inner.surfaces()
        )
    }
}
