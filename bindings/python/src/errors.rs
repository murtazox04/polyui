use crate::convert::to_py;
use crate::host::{operation_name, problems_value};
use polyui_core::CatalogError as CoreCatalogError;
use polyui_core::validate::Problems;
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;

create_exception!(polyui, PolyUIError, PyException);
create_exception!(polyui, ValidationError, PolyUIError);
create_exception!(polyui, CatalogError, PolyUIError);
create_exception!(polyui, ChartError, PolyUIError);
create_exception!(polyui, ProtocolError, PolyUIError);
create_exception!(polyui, RefRefusedError, PolyUIError);

pub fn validation_error(py: Python<'_>, problems: &Problems) -> PyErr {
    let error = ValidationError::new_err(problems.to_string());
    let attach = || -> PyResult<()> {
        let value = error.value(py);
        value.setattr("problems", to_py(py, &problems_value(problems))?)?;
        value.setattr("operation", operation_name(problems.operation))
    };
    match attach() {
        Ok(()) => error,
        Err(failure) => failure,
    }
}

pub fn catalog_error(py: Python<'_>, failure: &CoreCatalogError) -> PyErr {
    let error = CatalogError::new_err(failure.to_string());
    match error.value(py).setattr("path", failure.path.as_str()) {
        Ok(()) => error,
        Err(failure) => failure,
    }
}

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = module.py();
    module.add("PolyUIError", py.get_type::<PolyUIError>())?;
    module.add("ValidationError", py.get_type::<ValidationError>())?;
    module.add("CatalogError", py.get_type::<CatalogError>())?;
    module.add("ChartError", py.get_type::<ChartError>())?;
    module.add("ProtocolError", py.get_type::<ProtocolError>())?;
    module.add("RefRefusedError", py.get_type::<RefRefusedError>())?;
    Ok(())
}
