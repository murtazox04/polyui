use crate::catalogs::PyCatalogs;
use crate::convert::{to_json, to_py};
use crate::errors::{ChartError, catalog_error};
use crate::host::{self, chart_options, chart_spec};
use crate::surface::PySurface;
use polyui_chart::ChartSpec;
use polyui_chart::{Palette, compile, describe, to_svg};
use polyui_core::protocol::Component;
use polyui_core::summary;
use polyui_core::tools;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use serde_json::Value;

fn spec(props: &Bound<'_, PyAny>) -> PyResult<ChartSpec> {
    chart_spec(to_json(props)?)
        .map_err(|error| ChartError::new_err(format!("chart props could not be read: {error}")))
}

#[pyfunction]
#[pyo3(signature = (catalogs, actions=None, strict=false, profile=None))]
pub fn tool_definitions<'py>(
    py: Python<'py>,
    catalogs: &PyCatalogs,
    actions: Option<&Bound<'py, PyAny>>,
    strict: bool,
    profile: Option<&str>,
) -> PyResult<Bound<'py, PyAny>> {
    let options = host::tool_options(strict, profile).map_err(PyValueError::new_err)?;
    let spec = actions.map(to_json).transpose()?.unwrap_or(Value::Null);
    let table = host::action_table(&spec).map_err(|failure| catalog_error(py, &failure))?;
    to_py(
        py,
        &tools::tool_definitions(&catalogs.inner, &table, options),
    )
}

#[pyfunction]
#[pyo3(signature = (surface_id, surface, seq=1, fallback=None))]
pub fn open_message<'py>(
    py: Python<'py>,
    surface_id: &str,
    surface: &PySurface,
    seq: u64,
    fallback: Option<String>,
) -> PyResult<Bound<'py, PyAny>> {
    to_py(
        py,
        &host::open_message(surface_id, &surface.inner, seq, fallback),
    )
}

#[pyfunction]
#[pyo3(signature = (surface_id, seq, upsert=None, remove=Vec::new(), root=None))]
pub fn patch_message<'py>(
    py: Python<'py>,
    surface_id: &str,
    seq: u64,
    upsert: Option<&Bound<'py, PyAny>>,
    remove: Vec<String>,
    root: Option<String>,
) -> PyResult<Bound<'py, PyAny>> {
    let upsert = match upsert {
        Some(upsert) => {
            serde_json::from_value::<Vec<Component>>(to_json(upsert)?).map_err(|error| {
                PyTypeError::new_err(format!("upsert is not a list of components: {error}"))
            })?
        }
        None => Vec::new(),
    };
    to_py(
        py,
        &host::patch_message(surface_id, seq, upsert, remove, root),
    )
}

#[pyfunction]
pub fn close_message<'py>(
    py: Python<'py>,
    surface_id: &str,
    seq: u64,
) -> PyResult<Bound<'py, PyAny>> {
    to_py(py, &host::close_message(surface_id, seq))
}

#[pyfunction]
pub fn closed_summary(surface_id: &str) -> String {
    summary::closed(surface_id)
}

#[pyfunction]
#[pyo3(signature = (props, locale="en", width=None, height=None))]
pub fn compile_chart<'py>(
    py: Python<'py>,
    props: &Bound<'py, PyAny>,
    locale: &str,
    width: Option<u32>,
    height: Option<u32>,
) -> PyResult<Bound<'py, PyAny>> {
    let scene = compile(&spec(props)?, &chart_options(locale, width, height))
        .map_err(|error| ChartError::new_err(error.to_string()))?;
    to_py(py, &scene)
}

#[pyfunction]
#[pyo3(signature = (props, locale="en", width=None, height=None, dark=false))]
pub fn chart_svg(
    props: &Bound<'_, PyAny>,
    locale: &str,
    width: Option<u32>,
    height: Option<u32>,
    dark: bool,
) -> PyResult<String> {
    let scene = compile(&spec(props)?, &chart_options(locale, width, height))
        .map_err(|error| ChartError::new_err(error.to_string()))?;
    let palette = if dark {
        Palette::dark()
    } else {
        Palette::light()
    };
    Ok(to_svg(&scene, &palette))
}

#[pyfunction]
#[pyo3(signature = (props, locale="en"))]
pub fn describe_chart(props: &Bound<'_, PyAny>, locale: &str) -> PyResult<String> {
    Ok(describe(&spec(props)?, polyui_chart::Locale::parse(locale)))
}
