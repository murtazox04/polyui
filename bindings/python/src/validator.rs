use crate::catalogs::PyCatalogs;
use crate::convert::{input, to_json, to_py};
use crate::errors::{RefRefusedError, catalog_error, validation_error};
use crate::host::{
    self, Current, Input, Settings, SourceRefs, View, action_id, attach_apps, attach_scenes,
    chart_options,
};
use crate::surface::{PyPrepared, PySurface, PyUpdate, sealed_list};
use polyui_core::embed::UiResource;
use polyui_core::tools::{self, ToolOptions};
use polyui_core::validate::{Operation, Problems, is_valid_id};
use polyui_core::{Locale, RefResolver};
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyTuple;
use serde_json::Value;
use std::cell::RefCell;

struct Lookup<'a, 'py> {
    resolver: Option<&'a Bound<'py, PyAny>>,
    user: &'a Bound<'py, PyAny>,
    failure: RefCell<Option<PyErr>>,
}

impl<'a, 'py> Lookup<'a, 'py> {
    fn new(resolver: Option<&'a Bound<'py, PyAny>>, user: &'a Bound<'py, PyAny>) -> Self {
        Self {
            resolver,
            user,
            failure: RefCell::new(None),
        }
    }

    fn keep(&self, error: PyErr) {
        self.failure.borrow_mut().get_or_insert(error);
    }

    fn find(&self, source: &str, key: &str) -> Result<Value, String> {
        let Some(resolver) = self.resolver else {
            return Err("no resolver is configured".into());
        };
        let py = resolver.py();
        match resolver.call1((self.user, source, key)) {
            Ok(found) if found.is_none() => Err("no value".into()),
            Ok(found) if found.hasattr("__await__").unwrap_or(false) => {
                drop(found.call_method0("close"));
                self.keep(PyTypeError::new_err(
                    "the resolver returned an awaitable; it must return the value itself",
                ));
                Err("the resolver returned an awaitable".into())
            }
            Ok(found) => to_json(&found).map_err(|error| {
                self.keep(error);
                "the resolver returned data that is not JSON".to_string()
            }),
            Err(error) if error.is_instance_of::<RefRefusedError>(py) => {
                Err(error.value(py).to_string())
            }
            Err(error) => {
                self.keep(error);
                Err("the resolver failed".into())
            }
        }
    }

    fn finish<T>(self, py: Python<'_>, outcome: Result<T, Problems>) -> PyResult<T> {
        match self.failure.into_inner() {
            Some(error) => Err(error),
            None => outcome.map_err(|problems| validation_error(py, &problems)),
        }
    }
}

struct Apps<'a, 'py> {
    callback: Option<&'a Bound<'py, PyAny>>,
    user: &'a Bound<'py, PyAny>,
    failure: Option<PyErr>,
}

impl<'a, 'py> Apps<'a, 'py> {
    fn new(callback: Option<&'a Bound<'py, PyAny>>, user: &'a Bound<'py, PyAny>) -> Self {
        Self {
            callback,
            user,
            failure: None,
        }
    }

    fn keep(&mut self, error: PyErr) {
        self.failure.get_or_insert(error);
    }

    fn read(&mut self, uri: &str) -> Option<UiResource> {
        let callback = self.callback?;
        match callback.call1((self.user, uri)) {
            Ok(found) if found.is_none() => None,
            Ok(found) if found.hasattr("__await__").unwrap_or(false) => {
                drop(found.call_method0("close"));
                self.keep(PyTypeError::new_err(
                    "ui_resource returned an awaitable; it must return the resource itself",
                ));
                None
            }
            Ok(found) => {
                let resource = to_json(&found).and_then(|value| {
                    UiResource::from_value(value).map_err(|message| {
                        PyValueError::new_err(format!("ui_resource returned {message}"))
                    })
                });
                match resource {
                    Ok(resource) => Some(resource),
                    Err(error) => {
                        self.keep(error);
                        None
                    }
                }
            }
            Err(error) => {
                self.keep(error);
                None
            }
        }
    }

    fn finish(self) -> PyResult<()> {
        self.failure.map_or(Ok(()), Err)
    }
}

struct Ids<'a, 'py> {
    factory: Option<&'a Bound<'py, PyAny>>,
    failure: Option<PyErr>,
}

impl<'a, 'py> Ids<'a, 'py> {
    fn new(factory: Option<&'a Bound<'py, PyAny>>) -> Self {
        Self {
            factory,
            failure: None,
        }
    }

    fn next(&mut self) -> String {
        let Some(factory) = self.factory else {
            return action_id();
        };
        let made = factory.call0().and_then(|id| id.extract::<String>());
        match made {
            Ok(id) if is_valid_id(&id) => id,
            Ok(id) => {
                self.failure.get_or_insert_with(|| {
                    PyValueError::new_err(format!(
                        "next_id returned {id:?}; ids must be 1 to 64 characters from A-Z, a-z, 0-9, _ and -"
                    ))
                });
                action_id()
            }
            Err(error) => {
                self.failure.get_or_insert(error);
                action_id()
            }
        }
    }

    fn finish(self) -> PyResult<()> {
        self.failure.map_or(Ok(()), Err)
    }
}

#[pyclass(frozen, name = "Validator", module = "polyui")]
pub struct PyValidator {
    settings: Settings,
    resolver: Option<Py<PyAny>>,
    ui_resource: Option<Py<PyAny>>,
}

impl PyValidator {
    fn with_refs<'py, T>(
        &self,
        py: Python<'py>,
        user: &Bound<'py, PyAny>,
        run: impl FnOnce(&dyn RefResolver) -> Result<T, Problems>,
    ) -> PyResult<T> {
        let resolver = self.resolver.as_ref().map(|resolver| resolver.bind(py));
        let lookup = Lookup::new(resolver, user);
        let outcome = {
            let refs = SourceRefs::new(&self.settings.sources, |source, key| {
                lookup.find(source, key)
            });
            run(&refs)
        };
        lookup.finish(py, outcome)
    }

    fn surface_id(surface: &PySurface, surface_id: Option<&str>) -> String {
        surface_id
            .map(str::to_string)
            .or_else(|| surface.inner.id.clone())
            .unwrap_or_else(|| host::DEFAULT_SURFACE.to_string())
    }
}

#[pymethods]
impl PyValidator {
    #[new]
    #[pyo3(signature = (catalogs, actions=None, ref_sources=None, resolver=None, allow_http=false, ui_resource=None))]
    fn new(
        py: Python<'_>,
        catalogs: &PyCatalogs,
        actions: Option<&Bound<'_, PyAny>>,
        ref_sources: Option<&Bound<'_, PyAny>>,
        resolver: Option<Py<PyAny>>,
        allow_http: bool,
        ui_resource: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let data = |value: Option<&Bound<'_, PyAny>>| {
            value
                .map(to_json)
                .transpose()
                .map(Option::unwrap_or_default)
        };
        let actions =
            host::action_table(&data(actions)?).map_err(|failure| catalog_error(py, &failure))?;
        let sources = host::ref_sources(&data(ref_sources)?)
            .map_err(|failure| catalog_error(py, &failure))?;
        if let Some(resolver) = &resolver
            && !resolver.bind(py).is_callable()
        {
            return Err(PyTypeError::new_err("resolver must be callable"));
        }
        if let Some(ui_resource) = &ui_resource
            && !ui_resource.bind(py).is_callable()
        {
            return Err(PyTypeError::new_err("ui_resource must be callable"));
        }
        Ok(Self {
            settings: Settings {
                catalogs: catalogs.inner.clone(),
                actions,
                sources,
                allow_http,
            },
            resolver,
            ui_resource,
        })
    }

    #[getter]
    fn catalogs(&self) -> PyCatalogs {
        PyCatalogs {
            inner: self.settings.catalogs.clone(),
        }
    }

    #[getter]
    fn ref_sources(&self) -> Vec<String> {
        self.settings
            .sources
            .iter()
            .map(|(name, _)| name.clone())
            .collect()
    }

    fn show_ui(&self, py: Python<'_>, payload: &Bound<'_, PyAny>) -> PyResult<PySurface> {
        self.settings
            .show_ui(&input(payload)?)
            .map(PySurface::from)
            .map_err(|problems| validation_error(py, &problems))
    }

    fn update_ui(
        &self,
        py: Python<'_>,
        payload: &Bound<'_, PyAny>,
        current: &PySurface,
    ) -> PyResult<PyUpdate> {
        self.settings
            .update_ui(&input(payload)?, &current.inner)
            .map(|inner| PyUpdate { inner })
            .map_err(|problems| validation_error(py, &problems))
    }

    fn close_ui(&self, py: Python<'_>, payload: &Bound<'_, PyAny>) -> PyResult<String> {
        self.settings
            .close_ui(&input(payload)?)
            .map_err(|problems| validation_error(py, &problems))
    }

    #[pyo3(signature = (surface, user=None))]
    fn resolve_refs(
        &self,
        py: Python<'_>,
        surface: &PySurface,
        user: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PySurface> {
        let none = py.None().into_bound(py);
        let components = self.with_refs(py, user.unwrap_or(&none), |refs| {
            self.settings
                .resolve(&surface.inner.components, refs, Operation::Show)
        })?;
        let mut resolved = surface.inner.clone();
        resolved.components = components;
        Ok(resolved.into())
    }

    #[pyo3(signature = (surface, next_id=None))]
    fn seal_actions<'py>(
        &self,
        py: Python<'py>,
        surface: &PySurface,
        next_id: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyTuple>> {
        let mut ids = Ids::new(next_id);
        let mut sealed = surface.inner.clone();
        let actions = self
            .settings
            .seal(&mut sealed.components, &mut || ids.next());
        ids.finish()?;
        let list = sealed_list(py, &actions)?;
        PyTuple::new(
            py,
            [
                Bound::new(py, PySurface::from(sealed))?.into_any(),
                list.into_any(),
            ],
        )
    }

    #[pyo3(signature = (surface, locale="en", width=None, height=None))]
    fn compile_charts(
        &self,
        py: Python<'_>,
        surface: &PySurface,
        locale: &str,
        width: Option<u32>,
        height: Option<u32>,
    ) -> PyResult<PySurface> {
        let mut compiled = surface.inner.clone();
        let data = compiled.data_value();
        let problems = attach_scenes(
            &mut compiled.components,
            &data,
            "components",
            &chart_options(locale, width, height),
        );
        if problems.is_empty() {
            Ok(compiled.into())
        } else {
            Err(validation_error(
                py,
                &Problems {
                    operation: Operation::Show,
                    items: problems,
                },
            ))
        }
    }

    #[pyo3(signature = (surface, user=None))]
    fn attach_apps(
        &self,
        py: Python<'_>,
        surface: &PySurface,
        user: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PySurface> {
        let none = py.None().into_bound(py);
        let callback = self.ui_resource.as_ref().map(|callback| callback.bind(py));
        let mut apps = Apps::new(callback, user.unwrap_or(&none));
        let mut attached = surface.inner.clone();
        let problems = attach_apps(&mut attached.components, "components", &mut |uri| {
            apps.read(uri)
        });
        apps.finish()?;
        if problems.is_empty() {
            Ok(attached.into())
        } else {
            Err(validation_error(
                py,
                &Problems {
                    operation: Operation::Show,
                    items: problems,
                },
            ))
        }
    }

    #[pyo3(signature = (surface, declared, locale="en"))]
    fn downgrade(&self, surface: &PySurface, declared: Vec<String>, locale: &str) -> PySurface {
        self.settings
            .client_view(&surface.inner, Some(&declared), Locale::from_tag(locale))
            .into()
    }

    #[pyo3(signature = (surface, surface_id=None, locale="en"))]
    fn summary(&self, surface: &PySurface, surface_id: Option<&str>, locale: &str) -> String {
        surface.inner.summary(
            &Self::surface_id(surface, surface_id),
            &self.settings.catalogs,
            Locale::from_tag(locale),
        )
    }

    #[pyo3(signature = (surface, locale="en"))]
    fn fallback(&self, surface: &PySurface, locale: &str) -> String {
        surface
            .inner
            .fallback(&self.settings.catalogs, Locale::from_tag(locale))
    }

    #[pyo3(signature = (payload, user=None, surface_id=None, declared=None, locale="en", next_id=None))]
    #[allow(clippy::too_many_arguments)]
    fn prepare_show(
        &self,
        py: Python<'_>,
        payload: &Bound<'_, PyAny>,
        user: Option<&Bound<'_, PyAny>>,
        surface_id: Option<&str>,
        declared: Option<Vec<String>>,
        locale: &str,
        next_id: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyPrepared> {
        let payload = input(payload)?;
        let view = View::new(declared.as_deref(), locale);
        let none = py.None().into_bound(py);
        let mut ids = Ids::new(next_id);
        let user = user.unwrap_or(&none);
        let callback = self.ui_resource.as_ref().map(|callback| callback.bind(py));
        let mut apps = Apps::new(callback, user);
        let prepared = self.with_refs(py, user, |refs| {
            host::prepare_show(
                &self.settings,
                &payload,
                refs,
                surface_id,
                &view,
                &mut || ids.next(),
                &mut |uri| apps.read(uri),
            )
        });
        apps.finish()?;
        let prepared = prepared?;
        ids.finish()?;
        Ok(PyPrepared { inner: prepared })
    }

    #[pyo3(signature = (payload, model, client, seq, user=None, declared=None, locale="en", next_id=None))]
    #[allow(clippy::too_many_arguments)]
    fn prepare_update(
        &self,
        py: Python<'_>,
        payload: &Bound<'_, PyAny>,
        model: &PySurface,
        client: &PySurface,
        seq: u64,
        user: Option<&Bound<'_, PyAny>>,
        declared: Option<Vec<String>>,
        locale: &str,
        next_id: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyPrepared> {
        let payload: Input = input(payload)?;
        let view = View::new(declared.as_deref(), locale);
        let none = py.None().into_bound(py);
        let mut ids = Ids::new(next_id);
        let current = Current {
            model: &model.inner,
            client: &client.inner,
            seq,
        };
        let user = user.unwrap_or(&none);
        let callback = self.ui_resource.as_ref().map(|callback| callback.bind(py));
        let mut apps = Apps::new(callback, user);
        let prepared = self.with_refs(py, user, |refs| {
            host::prepare_update(
                &self.settings,
                &payload,
                current,
                refs,
                &view,
                &mut || ids.next(),
                &mut |uri| apps.read(uri),
            )
        });
        apps.finish()?;
        let prepared = prepared?;
        ids.finish()?;
        Ok(PyPrepared { inner: prepared })
    }

    #[pyo3(signature = (declared=None, strict=false))]
    fn tool_definitions<'py>(
        &self,
        py: Python<'py>,
        declared: Option<Vec<String>>,
        strict: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        let catalogs = match declared {
            Some(declared) => self.settings.catalogs.restrict(&declared),
            None => self.settings.catalogs.clone(),
        };
        to_py(
            py,
            &tools::tool_definitions(
                &catalogs,
                &self.settings.actions,
                ToolOptions {
                    strict,
                    ..ToolOptions::default()
                },
            ),
        )
    }

    fn __repr__(&self) -> String {
        format!(
            "Validator(catalogs={:?}, ref_sources={:?})",
            self.settings
                .catalogs
                .catalogs()
                .iter()
                .map(|catalog| catalog.tag())
                .collect::<Vec<_>>(),
            self.ref_sources()
        )
    }
}
