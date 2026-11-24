#[cfg(all(feature = "client-lite", any(feature = "tools", feature = "stream")))]
compile_error!("client-lite is the wasm client build, enable it without the default features");

pub mod catalog;
pub mod client;
mod codec;
mod decode;
mod fits;
#[cfg(any(test, feature = "client-lite"))]
mod json;
pub mod limits;
pub mod pointer;
pub mod protocol;
pub mod sanitize;
#[cfg(feature = "stream")]
pub mod stream;
#[cfg(not(feature = "client-lite"))]
pub mod summary;
pub mod text;
#[cfg(feature = "tools")]
pub mod tools;
#[cfg(not(feature = "client-lite"))]
pub mod validate;
#[cfg(feature = "client-lite")]
pub mod value;
mod view;
pub mod voice;

#[cfg(not(feature = "client-lite"))]
pub(crate) use serde_json::{Map, Value};
#[cfg(feature = "client-lite")]
pub(crate) use value::{Map, Value};

pub use catalog::{Catalog, CatalogError, CatalogSet, PropKind, PropSpec, TypeDef};
pub use client::{A11y, Client, RenderBatch, RenderOp};
#[cfg(feature = "client-lite")]
pub use codec::json_error;
pub use limits::{LimitError, Limits};
pub use protocol::{Component, DataOp, Incoming, Message, ProtocolError};
pub use text::Locale;
#[cfg(not(feature = "client-lite"))]
pub use validate::{
    ActionRegistry, ActionSpec, ActionTable, Operation, Problem, Problems, RefResolver,
    SealedAction, StaticRefs, Surface, Update, Validator,
};
pub use voice::{Utterance, VoiceMatch};
