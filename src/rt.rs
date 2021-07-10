use std::future::Future;

use once_cell::sync::Lazy;
use tokio::runtime::{Builder, Runtime};

static RUNTIME: Lazy<Runtime> =
    Lazy::new(|| Builder::new_multi_thread().enable_all().build().unwrap());

pub fn block_on<F: Future>(future: F) -> F::Output {
    RUNTIME.block_on(future)
}
