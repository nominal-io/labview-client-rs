//! One global multi-threaded Tokio runtime, created lazily on first use.
//!
//! Every FFI function that calls an `async fn` on the `nominal` crate runs it
//! through [`block_on`] — LabVIEW never sees async.

use once_cell::sync::Lazy;

static RUNTIME: Lazy<tokio::runtime::Runtime> =
    Lazy::new(|| tokio::runtime::Runtime::new().expect("failed to start Tokio runtime"));

/// Run an async operation to completion on the global runtime, blocking the
/// calling (LabVIEW) thread until it finishes.
pub(crate) fn block_on<F: std::future::Future>(future: F) -> F::Output {
    RUNTIME.block_on(future)
}
