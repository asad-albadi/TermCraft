use std::{future::Future, pin::Pin, time::Duration};
pub trait Clock: Send + Sync + 'static {
    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send>>;
}
pub struct TokioClock;
impl Clock for TokioClock {
    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(tokio::time::sleep(duration))
    }
}
