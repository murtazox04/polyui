mod catalogs;
mod client;
mod convert;
mod errors;
mod functions;
mod host;
mod stream;
mod surface;
mod validator;

use pyo3::prelude::*;

#[pymodule]
fn polyui(module: &Bound<'_, PyModule>) -> PyResult<()> {
    errors::register(module)?;
    module.add("__version__", env!("CARGO_PKG_VERSION"))?;
    module.add_class::<catalogs::PyCatalogs>()?;
    module.add_class::<validator::PyValidator>()?;
    module.add_class::<surface::PySurface>()?;
    module.add_class::<surface::PyUpdate>()?;
    module.add_class::<surface::PyPrepared>()?;
    module.add_class::<client::PyClient>()?;
    module.add_class::<stream::PyShowStream>()?;
    module.add_function(wrap_pyfunction!(functions::tool_definitions, module)?)?;
    module.add_function(wrap_pyfunction!(functions::open_message, module)?)?;
    module.add_function(wrap_pyfunction!(functions::patch_message, module)?)?;
    module.add_function(wrap_pyfunction!(functions::close_message, module)?)?;
    module.add_function(wrap_pyfunction!(functions::closed_summary, module)?)?;
    module.add_function(wrap_pyfunction!(functions::compile_chart, module)?)?;
    module.add_function(wrap_pyfunction!(functions::chart_svg, module)?)?;
    module.add_function(wrap_pyfunction!(functions::describe_chart, module)?)?;
    Ok(())
}
