use futures_util::StreamExt;
use futures_util::stream::{self, BoxStream};
use polyui_server::provider::ErrorKind;
use polyui_server::{ModelEvent, ModelProvider, ModelRequest, ProviderError, Usage};
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

pub const BUDGET_EXHAUSTED: &str = "the benchmark call budget is used up";

#[derive(Debug)]
pub struct Budget {
    used: AtomicUsize,
    max: usize,
}

impl Budget {
    pub fn new(max: usize) -> Arc<Self> {
        Arc::new(Self {
            used: AtomicUsize::new(0),
            max,
        })
    }

    pub fn used(&self) -> usize {
        self.used.load(Ordering::SeqCst)
    }

    fn take(&self) -> bool {
        let mut used = self.used.load(Ordering::SeqCst);
        while used < self.max {
            match self
                .used
                .compare_exchange(used, used + 1, Ordering::SeqCst, Ordering::SeqCst)
            {
                Ok(_) => return true,
                Err(actual) => used = actual,
            }
        }
        false
    }
}

#[derive(Debug, Default)]
pub struct Trace {
    pub requests: Vec<ModelRequest>,
    pub replies: Vec<Vec<Value>>,
    pub usage: Usage,
    pub refused: bool,
}

pub fn lock(trace: &Mutex<Trace>) -> MutexGuard<'_, Trace> {
    trace.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct Metered {
    inner: Arc<dyn ModelProvider>,
    budget: Arc<Budget>,
    trace: Arc<Mutex<Trace>>,
}

impl Metered {
    pub fn new(inner: Arc<dyn ModelProvider>, budget: Arc<Budget>) -> Self {
        Self {
            inner,
            budget,
            trace: Arc::new(Mutex::new(Trace::default())),
        }
    }

    pub fn trace(&self) -> Arc<Mutex<Trace>> {
        self.trace.clone()
    }
}

impl ModelProvider for Metered {
    fn stream(&self, request: ModelRequest) -> BoxStream<'static, ModelEvent> {
        if !self.budget.take() {
            lock(&self.trace).refused = true;
            return stream::iter(vec![ModelEvent::Error(ProviderError::new(
                ErrorKind::Other,
                BUDGET_EXHAUSTED,
            ))])
            .boxed();
        }
        lock(&self.trace).requests.push(request.clone());
        let trace = self.trace.clone();
        self.inner
            .stream(request)
            .inspect(move |event| {
                if let ModelEvent::MessageFinished { content, usage, .. } = event {
                    let mut trace = lock(&trace);
                    trace.replies.push(content.clone());
                    trace.usage.add(usage);
                }
            })
            .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use polyui_server::{ScriptedProvider, ScriptedReply};

    fn request() -> ModelRequest {
        ModelRequest {
            system: String::new(),
            tools: Vec::new(),
            messages: Vec::new(),
            max_tokens: 16,
        }
    }

    #[tokio::test]
    async fn the_budget_stops_calls_and_the_trace_records_replies() {
        let scripted = Arc::new(ScriptedProvider::new());
        scripted.reply(ScriptedReply::new().text("one"));
        scripted.reply(ScriptedReply::new().text("two"));
        let budget = Budget::new(1);
        let metered = Metered::new(scripted.clone(), budget.clone());
        let first: Vec<ModelEvent> = metered.stream(request()).collect().await;
        assert!(matches!(
            first.last(),
            Some(ModelEvent::MessageFinished { .. })
        ));
        let second: Vec<ModelEvent> = metered.stream(request()).collect().await;
        assert!(matches!(second.as_slice(), [ModelEvent::Error(_)]));
        assert_eq!(budget.used(), 1);
        assert_eq!(scripted.requests().len(), 1);
        let trace = metered.trace();
        let trace = lock(&trace);
        assert_eq!(trace.requests.len(), 1);
        assert_eq!(trace.replies.len(), 1);
        assert!(trace.refused);
    }
}
