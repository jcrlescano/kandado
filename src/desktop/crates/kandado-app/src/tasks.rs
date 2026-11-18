//! Background work. The views run on GPUI's executor, which does not know
//! about Tokio, so anything that talks to the network or derives a key runs
//! here and the view awaits the answer.

use std::{future::Future, sync::Arc};

use tokio::runtime::Runtime;

/// A Tokio runtime shared by the whole app.
#[derive(Clone)]
pub struct Tasks {
    runtime: Arc<Runtime>,
}

impl Tasks {
    /// The runtime, for services that run on it for the app's lifetime.
    pub fn handle(&self) -> tokio::runtime::Handle {
        self.runtime.handle().clone()
    }

    pub fn new() -> std::io::Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("kandado-tasks")
            .enable_all()
            .build()?;
        Ok(Self {
            runtime: Arc::new(runtime),
        })
    }

    /// Runs `work` on the runtime. The returned future can be awaited from
    /// any executor.
    pub fn run<T: Send + 'static, F: Future<Output = T> + Send + 'static>(
        &self,
        work: F,
    ) -> impl Future<Output = T> + use<T, F> {
        let handle = self.runtime.spawn(work);
        async move { handle.await.expect("a background task panicked") }
    }
}

impl Tasks {
    /// Like [`Tasks::run`], but the work can be stopped. The future gives
    /// `None` if it was cancelled.
    pub fn run_cancellable<T: Send + 'static, F: Future<Output = T> + Send + 'static>(
        &self,
        work: F,
    ) -> (impl Future<Output = Option<T>> + use<T, F>, Canceller) {
        let handle = self.runtime.spawn(work);
        let canceller = Canceller(handle.abort_handle());
        let future = async move {
            match handle.await {
                Ok(value) => Some(value),
                Err(error) if error.is_cancelled() => None,
                Err(error) => panic!("a background task panicked: {error}"),
            }
        };
        (future, canceller)
    }
}

impl Tasks {
    /// Runs `work` on the runtime and does not wait for it.
    pub fn spawn_detached<F: Future<Output = ()> + Send + 'static>(&self, work: F) {
        self.runtime.spawn(work);
    }
}

/// Stops a task started with [`Tasks::run_cancellable`].
#[derive(Clone)]
pub struct Canceller(tokio::task::AbortHandle);

impl Canceller {
    pub fn cancel(&self) {
        self.0.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelled_work_gives_none() {
        let tasks = Tasks::new().unwrap();
        let (future, canceller) = tasks.run_cancellable(std::future::pending::<u8>());
        canceller.cancel();
        let result = tasks.runtime.block_on(future);
        assert_eq!(result, None);
    }

    #[test]
    fn work_runs_on_the_runtime_and_can_be_awaited_elsewhere() {
        let tasks = Tasks::new().unwrap();
        let answer = tasks.run(async {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            42
        });
        // Poll from a plain thread with no Tokio context.
        let value = std::thread::spawn(move || {
            let (sender, receiver) = std::sync::mpsc::channel();
            let waker = std::task::Waker::noop();
            let mut cx = std::task::Context::from_waker(waker);
            let mut answer = Box::pin(answer);
            loop {
                if let std::task::Poll::Ready(value) = answer.as_mut().poll(&mut cx) {
                    sender.send(value).unwrap();
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            receiver.recv().unwrap()
        })
        .join()
        .unwrap();
        assert_eq!(value, 42);
    }
}
