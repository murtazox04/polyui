use crate::host::Input;
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::PyString;
use pythonize::{depythonize, pythonize};
use serde::Serialize;
use serde_json::Value;

pub fn to_py<'py, T: Serialize + ?Sized>(
    py: Python<'py>,
    value: &T,
) -> PyResult<Bound<'py, PyAny>> {
    Ok(pythonize(py, value)?)
}

pub fn to_json(value: &Bound<'_, PyAny>) -> PyResult<Value> {
    depythonize::<Value>(value)
        .map_err(|error| PyTypeError::new_err(format!("expected JSON-compatible data: {error}")))
}

pub fn input(value: &Bound<'_, PyAny>) -> PyResult<Input> {
    match value.cast::<PyString>() {
        Ok(text) => Ok(Input::Text(text.to_str()?.to_owned())),
        Err(_) => to_json(value).map(Input::Json),
    }
}

pub fn json_text(value: &Bound<'_, PyAny>) -> PyResult<String> {
    match value.cast::<PyString>() {
        Ok(text) => Ok(text.to_str()?.to_owned()),
        Err(_) => Ok(to_json(value)?.to_string()),
    }
}
