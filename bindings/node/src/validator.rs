use crate::catalogs::Catalogs;
use crate::errors::{catalog_error, is_refusal, is_thenable, validation_error};
use crate::host::{
    self, Current, Input, Settings, SourceRefs, View, action_id, attach_apps, attach_scenes,
    chart_options,
};
use crate::surface::{Prepared, Surface, Update, sealed_list};
use napi::bindgen_prelude::{FnArgs, Function, FunctionRef, Unknown};
use napi::{Env, Error, JsError, Result, Status, ValueType};
use napi_derive::napi;
use polyui_core::embed::UiResource;
use polyui_core::tools;
use polyui_core::validate::{Operation, Problems, is_valid_id};
use polyui_core::{Locale, RefResolver};
use serde_json::{Map, Value};
use std::cell::RefCell;

type ResolverArgs<'a> = FnArgs<(Option<Unknown<'a>>, String, String)>;
type StoredResolver = FunctionRef<ResolverArgs<'static>, Unknown<'static>>;
type ReaderArgs<'a> = FnArgs<(Option<Unknown<'a>>, String)>;
type StoredReader = FunctionRef<ReaderArgs<'static>, Unknown<'static>>;

#[napi(object, object_to_js = false)]
pub struct ValidatorOptions {
    pub actions: Option<Value>,
    pub ref_sources: Option<Value>,
    pub resolver: Option<StoredResolver>,
    pub allow_http: Option<bool>,
    pub ui_resource: Option<StoredReader>,
}

#[napi(object, object_to_js = false)]
pub struct PrepareOptions<'a> {
    pub user: Option<Unknown<'a>>,
    pub surface_id: Option<String>,
    pub declared: Option<Vec<String>>,
    pub locale: Option<String>,
    pub next_id: Option<Function<'a, (), String>>,
    pub refs: Option<Value>,
}

#[napi(object, object_to_js = false)]
pub struct ChartSize {
    pub locale: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[napi(object, object_from_js = false)]
pub struct Sealed {
    pub surface: Surface,
    pub actions: Value,
}

struct Lookup<'a> {
    env: &'a Env,
    resolver: Option<Function<'a, ResolverArgs<'a>, Unknown<'a>>>,
    given: Option<&'a Map<String, Value>>,
    user: Option<Unknown<'a>>,
    failure: RefCell<Option<Error>>,
}

impl<'a> Lookup<'a> {
    fn new(
        env: &'a Env,
        stored: Option<&StoredResolver>,
        given: Option<&'a Map<String, Value>>,
        user: Option<Unknown<'a>>,
    ) -> Result<Self> {
        let resolver = match stored {
            Some(stored) => {
                let function: Function<'a, ResolverArgs<'a>, Unknown<'a>> =
                    stored.borrow_back(env)?;
                Some(function)
            }
            None => None,
        };
        Ok(Self {
            env,
            resolver,
            given,
            user,
            failure: RefCell::new(None),
        })
    }

    fn keep(&self, error: Error) {
        self.failure.borrow_mut().get_or_insert(error);
    }

    fn find(&self, source: &str, key: &str) -> std::result::Result<Value, String> {
        if let Some(given) = self.given {
            return host::given_ref(given, source, key);
        }
        let Some(resolver) = &self.resolver else {
            return Err("no resolver is configured".into());
        };
        match resolver.call(FnArgs::from((
            self.user,
            source.to_string(),
            key.to_string(),
        ))) {
            Ok(found) => match found.get_type() {
                Ok(ValueType::Undefined | ValueType::Null) => Err("no value".into()),
                _ if is_thenable(&found) => {
                    self.keep(Error::new(
                        Status::InvalidArg,
                        "the resolver returned a Promise; it must return the value itself",
                    ));
                    Err("the resolver returned a Promise".into())
                }
                _ => self.env.from_js_value::<Value, _>(found).map_err(|error| {
                    self.keep(error);
                    "the resolver returned data that is not JSON".to_string()
                }),
            },
            Err(error) => {
                let thrown = JsError::from(error).into_unknown(*self.env);
                if is_refusal(&thrown) {
                    Err("refused".into())
                } else {
                    self.keep(Error::from(thrown));
                    Err("the resolver failed".into())
                }
            }
        }
    }

    fn finish<T>(self, outcome: std::result::Result<T, Problems>) -> Result<T> {
        match self.failure.into_inner() {
            Some(error) => Err(error),
            None => outcome.map_err(|problems| validation_error(self.env, &problems)),
        }
    }
}

struct Apps<'a> {
    env: &'a Env,
    reader: Option<Function<'a, ReaderArgs<'a>, Unknown<'a>>>,
    user: Option<Unknown<'a>>,
    failure: Option<Error>,
}

impl<'a> Apps<'a> {
    fn new(env: &'a Env, stored: Option<&StoredReader>, user: Option<Unknown<'a>>) -> Result<Self> {
        let reader = match stored {
            Some(stored) => {
                let function: Function<'a, ReaderArgs<'a>, Unknown<'a>> =
                    stored.borrow_back(env)?;
                Some(function)
            }
            None => None,
        };
        Ok(Self {
            env,
            reader,
            user,
            failure: None,
        })
    }

    fn keep(&mut self, error: Error) {
        self.failure.get_or_insert(error);
    }

    fn read(&mut self, uri: &str) -> Option<UiResource> {
        let reader = self.reader.as_ref()?;
        let found = match reader.call(FnArgs::from((self.user, uri.to_string()))) {
            Ok(found) => found,
            Err(error) => {
                self.keep(error);
                return None;
            }
        };
        match found.get_type() {
            Ok(ValueType::Undefined | ValueType::Null) => None,
            _ if is_thenable(&found) => {
                self.keep(Error::new(
                    Status::InvalidArg,
                    "uiResource returned a Promise; it must return the resource itself",
                ));
                None
            }
            _ => {
                let resource = self.env.from_js_value::<Value, _>(found).and_then(|value| {
                    UiResource::from_value(value).map_err(|message| {
                        Error::new(Status::InvalidArg, format!("uiResource returned {message}"))
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
        }
    }

    fn finish(self) -> Result<()> {
        self.failure.map_or(Ok(()), Err)
    }
}

struct Ids<'a> {
    factory: Option<Function<'a, (), String>>,
    failure: Option<Error>,
}

impl<'a> Ids<'a> {
    fn new(factory: Option<Function<'a, (), String>>) -> Self {
        Self {
            factory,
            failure: None,
        }
    }

    fn next(&mut self) -> String {
        let Some(factory) = &self.factory else {
            return action_id();
        };
        match factory.call(()) {
            Ok(id) if is_valid_id(&id) => id,
            Ok(id) => {
                self.failure.get_or_insert_with(|| {
                    Error::new(
                        Status::InvalidArg,
                        format!(
                            "nextId returned {id:?}; ids must be 1 to 64 characters from A-Z, a-z, 0-9, _ and -"
                        ),
                    )
                });
                action_id()
            }
            Err(error) => {
                self.failure.get_or_insert(error);
                action_id()
            }
        }
    }

    fn finish(self) -> Result<()> {
        self.failure.map_or(Ok(()), Err)
    }
}

fn input(value: Value) -> Input {
    match value {
        Value::String(text) => Input::Text(text),
        other => Input::Json(other),
    }
}

#[napi]
pub struct Validator {
    settings: Settings,
    resolver: Option<StoredResolver>,
    ui_resource: Option<StoredReader>,
}

impl Validator {
    fn with_refs<'a, T>(
        &self,
        env: &'a Env,
        user: Option<Unknown<'a>>,
        given: Option<&'a Map<String, Value>>,
        run: impl FnOnce(&dyn RefResolver) -> std::result::Result<T, Problems>,
    ) -> Result<T> {
        let lookup = Lookup::new(env, self.resolver.as_ref(), given, user)?;
        let outcome = {
            let refs = SourceRefs::new(&self.settings.sources, |source, key| {
                lookup.find(source, key)
            });
            run(&refs)
        };
        lookup.finish(outcome)
    }
}

fn given(refs: Option<Value>) -> Result<Option<Map<String, Value>>> {
    match refs {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(map)) => Ok(Some(map)),
        Some(_) => Err(Error::new(
            Status::InvalidArg,
            "refs must be an object from \"source:key\" to a value",
        )),
    }
}

#[napi]
impl Validator {
    #[napi(constructor)]
    pub fn new(env: Env, catalogs: &Catalogs, options: Option<ValidatorOptions>) -> Result<Self> {
        let options = options.unwrap_or(ValidatorOptions {
            actions: None,
            ref_sources: None,
            resolver: None,
            allow_http: None,
            ui_resource: None,
        });
        let actions = host::action_table(&options.actions.unwrap_or_default())
            .map_err(|failure| catalog_error(&env, &failure))?;
        let sources = host::ref_sources(&options.ref_sources.unwrap_or_default())
            .map_err(|failure| catalog_error(&env, &failure))?;
        Ok(Self {
            settings: Settings {
                catalogs: catalogs.inner.clone(),
                actions,
                sources,
                allow_http: options.allow_http.unwrap_or(false),
            },
            resolver: options.resolver,
            ui_resource: options.ui_resource,
        })
    }

    #[napi(getter)]
    pub fn catalogs(&self) -> Catalogs {
        Catalogs {
            inner: self.settings.catalogs.clone(),
        }
    }

    #[napi(getter)]
    pub fn ref_sources(&self) -> Vec<String> {
        self.settings
            .sources
            .iter()
            .map(|(name, _)| name.clone())
            .collect()
    }

    #[napi]
    pub fn show_ui(&self, env: Env, payload: Value) -> Result<Surface> {
        self.settings
            .show_ui(&input(payload))
            .map(Surface::from)
            .map_err(|problems| validation_error(&env, &problems))
    }

    #[napi]
    pub fn update_ui(&self, env: Env, payload: Value, current: &Surface) -> Result<Update> {
        self.settings
            .update_ui(&input(payload), &current.inner)
            .map(|inner| Update { inner })
            .map_err(|problems| validation_error(&env, &problems))
    }

    #[napi]
    pub fn close_ui(&self, env: Env, payload: Value) -> Result<String> {
        self.settings
            .close_ui(&input(payload))
            .map_err(|problems| validation_error(&env, &problems))
    }

    #[napi]
    pub fn resolve_refs<'a>(
        &self,
        env: &'a Env,
        surface: &Surface,
        user: Option<Unknown<'a>>,
        refs: Option<Value>,
    ) -> Result<Surface> {
        let given = given(refs)?;
        let components = self.with_refs(env, user, given.as_ref(), |refs| {
            self.settings
                .resolve(&surface.inner.components, refs, Operation::Show)
        })?;
        let mut resolved = surface.inner.clone();
        resolved.components = components;
        Ok(resolved.into())
    }

    #[napi]
    pub fn seal_actions(
        &self,
        surface: &Surface,
        next_id: Option<Function<'_, (), String>>,
    ) -> Result<Sealed> {
        let mut ids = Ids::new(next_id);
        let mut sealed = surface.inner.clone();
        let actions = self
            .settings
            .seal(&mut sealed.components, &mut || ids.next());
        ids.finish()?;
        Ok(Sealed {
            surface: sealed.into(),
            actions: sealed_list(&actions),
        })
    }

    #[napi]
    pub fn compile_charts(
        &self,
        env: Env,
        surface: &Surface,
        options: Option<ChartSize>,
    ) -> Result<Surface> {
        let size = options.unwrap_or(ChartSize {
            locale: None,
            width: None,
            height: None,
        });
        let mut compiled = surface.inner.clone();
        let data = compiled.data_value();
        let chart = chart_options(
            size.locale.as_deref().unwrap_or("en"),
            size.width,
            size.height,
        );
        let problems = attach_scenes(&mut compiled.components, &data, "components", &chart);
        if problems.is_empty() {
            Ok(compiled.into())
        } else {
            Err(validation_error(
                &env,
                &Problems {
                    operation: Operation::Show,
                    items: problems,
                },
            ))
        }
    }

    #[napi]
    pub fn attach_apps<'a>(
        &self,
        env: &'a Env,
        surface: &Surface,
        user: Option<Unknown<'a>>,
    ) -> Result<Surface> {
        let mut apps = Apps::new(env, self.ui_resource.as_ref(), user)?;
        let mut attached = surface.inner.clone();
        let problems = attach_apps(&mut attached.components, "components", &mut |uri| {
            apps.read(uri)
        });
        apps.finish()?;
        if problems.is_empty() {
            Ok(attached.into())
        } else {
            Err(validation_error(
                env,
                &Problems {
                    operation: Operation::Show,
                    items: problems,
                },
            ))
        }
    }

    #[napi]
    pub fn downgrade(
        &self,
        surface: &Surface,
        declared: Vec<String>,
        locale: Option<String>,
    ) -> Surface {
        self.settings
            .client_view(
                &surface.inner,
                Some(&declared),
                Locale::from_tag(locale.as_deref().unwrap_or("en")),
            )
            .into()
    }

    #[napi]
    pub fn summary(
        &self,
        surface: &Surface,
        surface_id: Option<String>,
        locale: Option<String>,
    ) -> String {
        let id = surface_id
            .or_else(|| surface.inner.id.clone())
            .unwrap_or_else(|| host::DEFAULT_SURFACE.to_string());
        surface.inner.summary(
            &id,
            &self.settings.catalogs,
            Locale::from_tag(locale.as_deref().unwrap_or("en")),
        )
    }

    #[napi]
    pub fn fallback(&self, surface: &Surface, locale: Option<String>) -> String {
        surface.inner.fallback(
            &self.settings.catalogs,
            Locale::from_tag(locale.as_deref().unwrap_or("en")),
        )
    }

    #[napi]
    pub fn prepare_show<'a>(
        &self,
        env: &'a Env,
        payload: Value,
        options: Option<PrepareOptions<'a>>,
    ) -> Result<Prepared> {
        let options = options.unwrap_or(PrepareOptions {
            user: None,
            surface_id: None,
            declared: None,
            locale: None,
            next_id: None,
            refs: None,
        });
        let payload = input(payload);
        let given = given(options.refs)?;
        let locale = options.locale.unwrap_or_else(|| "en".into());
        let view = View::new(options.declared.as_deref(), &locale);
        let mut ids = Ids::new(options.next_id);
        let surface_id = options.surface_id;
        let mut apps = Apps::new(env, self.ui_resource.as_ref(), options.user)?;
        let prepared = self.with_refs(env, options.user, given.as_ref(), |refs| {
            host::prepare_show(
                &self.settings,
                &payload,
                refs,
                surface_id.as_deref(),
                &view,
                &mut || ids.next(),
                &mut |uri| apps.read(uri),
            )
        });
        apps.finish()?;
        let prepared = prepared?;
        ids.finish()?;
        Ok(Prepared { inner: prepared })
    }

    #[napi]
    pub fn prepare_update<'a>(
        &self,
        env: &'a Env,
        payload: Value,
        model: &Surface,
        client: &Surface,
        seq: u32,
        options: Option<PrepareOptions<'a>>,
    ) -> Result<Prepared> {
        let options = options.unwrap_or(PrepareOptions {
            user: None,
            surface_id: None,
            declared: None,
            locale: None,
            next_id: None,
            refs: None,
        });
        let payload = input(payload);
        let given = given(options.refs)?;
        let locale = options.locale.unwrap_or_else(|| "en".into());
        let view = View::new(options.declared.as_deref(), &locale);
        let mut ids = Ids::new(options.next_id);
        let current = Current {
            model: &model.inner,
            client: &client.inner,
            seq: u64::from(seq),
        };
        let mut apps = Apps::new(env, self.ui_resource.as_ref(), options.user)?;
        let prepared = self.with_refs(env, options.user, given.as_ref(), |refs| {
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
        Ok(Prepared { inner: prepared })
    }

    #[napi]
    pub fn ref_keys(&self, payload: Value) -> Vec<String> {
        self.settings.ref_keys(&input(payload))
    }

    #[napi]
    pub fn tool_definitions(
        &self,
        declared: Option<Vec<String>>,
        strict: Option<bool>,
        profile: Option<String>,
    ) -> Result<Value> {
        let options = host::tool_options(strict.unwrap_or(false), profile.as_deref())
            .map_err(|message| Error::new(Status::InvalidArg, message))?;
        let catalogs = match declared {
            Some(declared) => self.settings.catalogs.restrict(&declared),
            None => self.settings.catalogs.clone(),
        };
        Ok(tools::tool_definitions(
            &catalogs,
            &self.settings.actions,
            options,
        ))
    }
}
