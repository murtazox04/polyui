use crate::convert::to_py;
use crate::host::ShowStream;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

#[pyclass(name = "ShowStream", module = "polyui")]
pub struct PyShowStream {
    pub inner: ShowStream,
    pub finished: bool,
}

impl PyShowStream {
    pub fn live(&mut self) -> PyResult<&mut ShowStream> {
        if self.finished {
            return Err(PyRuntimeError::new_err("this stream is already finished"));
        }
        Ok(&mut self.inner)
    }
}

#[pymethods]
impl PyShowStream {
    #[new]
    fn new() -> Self {
        Self {
            inner: ShowStream::default(),
            finished: false,
        }
    }

    #[getter]
    fn opened(&self) -> Option<String> {
        self.inner.opened().map(str::to_string)
    }

    #[getter]
    fn seq(&self) -> u64 {
        self.inner.seq()
    }

    #[getter]
    fn stopped(&self) -> bool {
        self.inner.stopped()
    }

    #[getter]
    fn finished(&self) -> bool {
        self.finished
    }

    #[getter]
    fn text(&self) -> String {
        self.inner.text().to_string()
    }

    fn close_message<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
        self.inner
            .close_message()
            .map(|message| to_py(py, &message))
            .transpose()
    }

    fn __repr__(&self) -> String {
        let opened = self
            .inner
            .opened()
            .map_or_else(|| "None".to_string(), |id| format!("{id:?}"));
        let flag = |value: bool| if value { "True" } else { "False" };
        format!(
            "ShowStream(opened={opened}, seq={}, stopped={}, finished={})",
            self.inner.seq(),
            flag(self.inner.stopped()),
            flag(self.finished)
        )
    }
}
