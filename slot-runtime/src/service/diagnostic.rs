use std::sync::Arc;

mod context;
mod model;

pub use context::OperationContext;
pub use model::{DiagnosticCause, DiagnosticCommand, DiagnosticFailure, OperationPhase};

#[doc = "Receives structured internal failures without changing wire behavior."]
pub trait DiagnosticSink: core::fmt::Debug + Send + Sync {
    #[doc = "Records one failure; implementations should be bounded and non-panicking."]
    fn record_failure(&self, failure: DiagnosticFailure);
}

#[doc = "Default no-op diagnostic sink."]
#[derive(Debug, Default)]
pub struct NoopDiagnosticSink;

impl DiagnosticSink for NoopDiagnosticSink {
    fn record_failure(&self, _failure: DiagnosticFailure) {}
}

impl<T> DiagnosticSink for Arc<T>
where
    T: DiagnosticSink + ?Sized,
{
    fn record_failure(&self, failure: DiagnosticFailure) {
        (**self).record_failure(failure);
    }
}
