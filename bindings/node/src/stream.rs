use crate::host;
use napi_derive::napi;
use serde_json::Value;

#[napi]
pub struct ShowStream {
    pub(crate) inner: host::ShowStream,
    pub(crate) finished: bool,
}

impl Default for ShowStream {
    fn default() -> Self {
        Self::new()
    }
}

#[napi]
impl ShowStream {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: host::ShowStream::default(),
            finished: false,
        }
    }

    #[napi(getter)]
    pub fn opened(&self) -> Option<String> {
        self.inner.opened().map(str::to_string)
    }

    #[napi(getter)]
    pub fn seq(&self) -> u32 {
        u32::try_from(self.inner.seq()).unwrap_or(u32::MAX)
    }

    #[napi(getter)]
    pub fn stopped(&self) -> bool {
        self.inner.stopped()
    }

    #[napi(getter)]
    pub fn finished(&self) -> bool {
        self.finished
    }

    #[napi(getter)]
    pub fn text(&self) -> String {
        self.inner.text().to_string()
    }

    #[napi]
    pub fn close_message(&self) -> Option<Value> {
        self.inner.close_message()
    }
}
