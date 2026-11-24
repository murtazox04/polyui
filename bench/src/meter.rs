use futures_util::StreamExt;
use futures_util::stream::{self, BoxStream};
use polyui_server::provider::ErrorKind;
use polyui_server::{ModelEvent, ModelProvider, ModelRequest, ProviderError, Usage};
use serde_json::Value;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;
use tokio::time::Instant;

pub const BUDGET_EXHAUSTED: &str = "the benchmark call budget is used up";

#[derive(Debug)]
pub struct Budget {
    used: AtomicUsize,
    max: usize,
    errors_in_a_row: AtomicUsize,
    stop_after_errors: usize,
    stopped: AtomicBool,
}

impl Budget {
    pub fn new(max: usize, errors: usize) -> Arc<Self> {
        Arc::new(Self {
            used: AtomicUsize::new(0),
            max,
            errors_in_a_row: AtomicUsize::new(0),
            stop_after_errors: errors,
            stopped: AtomicBool::new(false),
        })
    }

    pub fn used(&self) -> usize {
        self.used.load(Ordering::SeqCst)
    }

    pub fn stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    fn note(&self, failed: bool) {
        if !failed {
            self.errors_in_a_row.store(0, Ordering::SeqCst);
            return;
        }
        let errors = self.errors_in_a_row.fetch_add(1, Ordering::SeqCst) + 1;
        if self.stop_after_errors > 0 && errors >= self.stop_after_errors {
            self.stopped.store(true, Ordering::SeqCst);
        }
    }

    fn take(&self) -> bool {
        if self.stopped() {
            return false;
        }
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
    pub errors: Vec<String>,
}

pub fn lock(trace: &Mutex<Trace>) -> MutexGuard<'_, Trace> {
    trace.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Debug)]
pub struct Pacer {
    gap: Duration,
    next: tokio::sync::Mutex<Option<Instant>>,
}

impl Pacer {
    pub fn new(gap: Duration) -> Arc<Self> {
        Arc::new(Self {
            gap,
            next: tokio::sync::Mutex::new(None),
        })
    }

    pub async fn wait(&self) {
        let mut next = self.next.lock().await;
        if let Some(at) = *next {
            tokio::time::sleep_until(at).await;
        }
        *next = Some(Instant::now() + self.gap);
    }
}

pub struct Metered {
    inner: Arc<dyn ModelProvider>,
    budget: Arc<Budget>,
    pacer: Arc<Pacer>,
    trace: Arc<Mutex<Trace>>,
}

impl Metered {
    pub fn new(inner: Arc<dyn ModelProvider>, budget: Arc<Budget>, pacer: Arc<Pacer>) -> Self {
        Self {
            inner,
            budget,
            pacer,
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
        let budget = self.budget.clone();
        let pacer = self.pacer.clone();
        let inner = self.inner.clone();
        stream::once(async move {
            pacer.wait().await;
            inner.stream(request)
        })
        .flatten()
        .inspect(move |event| match event {
            ModelEvent::MessageFinished { content, usage, .. } => {
                budget.note(false);
                let mut trace = lock(&trace);
                trace.replies.push(content.clone());
                trace.usage.add(usage);
            }
            ModelEvent::Error(error) => {
                budget.note(true);
                lock(&trace).errors.push(error.detail.clone());
            }
            _ => {}
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
        let budget = Budget::new(1, 0);
        let metered = Metered::new(scripted.clone(), budget.clone(), Pacer::new(Duration::ZERO));
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

    #[tokio::test]
    async fn errors_in_a_row_stop_the_run_and_a_success_resets_the_count() {
        let scripted = Arc::new(ScriptedProvider::new());
        let failure = || ProviderError::new(ErrorKind::Overloaded, "HTTP 502: upstream");
        scripted.reply(ScriptedReply::new().error(failure()));
        scripted.reply(ScriptedReply::new().text("fine"));
        scripted.reply(ScriptedReply::new().error(failure()));
        scripted.reply(ScriptedReply::new().error(failure()));
        let budget = Budget::new(10, 2);
        let metered = Metered::new(scripted.clone(), budget.clone(), Pacer::new(Duration::ZERO));
        for _ in 0..4 {
            let _: Vec<ModelEvent> = metered.stream(request()).collect().await;
        }
        assert!(budget.stopped());
        assert_eq!(lock(&metered.trace()).errors.len(), 3);
        let refused: Vec<ModelEvent> = metered.stream(request()).collect().await;
        assert!(matches!(refused.as_slice(), [ModelEvent::Error(_)]));
        assert_eq!(budget.used(), 4);
    }

    #[tokio::test(start_paused = true)]
    async fn the_pacer_keeps_a_gap_between_calls() {
        let pacer = Pacer::new(Duration::from_secs(3));
        let started = Instant::now();
        pacer.wait().await;
        pacer.wait().await;
        pacer.wait().await;
        assert!(started.elapsed() >= Duration::from_secs(6));
    }
}
