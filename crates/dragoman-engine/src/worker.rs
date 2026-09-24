// SPDX-License-Identifier: GPL-3.0-or-later

//! One dedicated OS thread per loaded language pair. The thread owns the
//! backend and its model(s), takes jobs from a channel and answers through
//! oneshot channels; the engine is never called from an async task.
//!
//! Unloading a pair is dropping its [`Worker`]: the channel closes, the
//! thread finishes the jobs already queued and exits, dropping the models.

use std::sync::mpsc;
use std::thread;

use tokio::sync::oneshot;

use crate::backend::{Backend, Error, ModelFiles, Result, TranslateOptions};

/// What one worker loads: a direct pair, or two legs of a pivot route.
pub struct ModelSpec {
    pub files: ModelFiles,
    /// Path-free Marian options; see [`crate::marian_config`].
    pub config_yaml: String,
}

struct Job {
    segments: Vec<String>,
    options: TranslateOptions,
    reply: oneshot::Sender<Result<Vec<String>>>,
}

/// Handle to a worker thread. Cheap to use from async code.
pub struct Worker {
    jobs: mpsc::Sender<Job>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Worker {
    /// Spawns a worker that loads `first` (and `second` for a pivot route)
    /// on its own thread. The returned receiver resolves once loading
    /// finished; jobs submitted earlier wait in the queue.
    pub fn spawn<B: Backend>(
        mut backend: B,
        first: ModelSpec,
        second: Option<ModelSpec>,
    ) -> (Worker, oneshot::Receiver<Result<()>>) {
        let (job_sender, job_receiver) = mpsc::channel::<Job>();
        let (ready_sender, ready_receiver) = oneshot::channel();

        let thread = thread::Builder::new()
            .name("dragoman-worker".into())
            .spawn(move || {
                let load = |backend: &mut B, spec: &ModelSpec| {
                    backend.load(&spec.files, &spec.config_yaml)
                };
                let models = (|| {
                    let first_model = load(&mut backend, &first)?;
                    let second_model = second
                        .as_ref()
                        .map(|spec| load(&mut backend, spec))
                        .transpose()?;
                    Ok((first_model, second_model))
                })();

                let (first_model, second_model) = match models {
                    Ok(models) => {
                        let _ = ready_sender.send(Ok(()));
                        models
                    }
                    Err(error) => {
                        let _ = ready_sender.send(Err(error));
                        // Answer everything already queued, then quit.
                        for job in job_receiver.iter() {
                            let _ = job.reply.send(Err(Error::WorkerGone));
                        }
                        return;
                    }
                };

                for job in job_receiver.iter() {
                    let result = backend.translate(
                        &first_model,
                        second_model.as_ref(),
                        job.segments,
                        job.options,
                    );
                    // The requester may be gone (cancelled); that is fine.
                    let _ = job.reply.send(result);
                }
                // Channel closed: models and backend drop here, on this
                // thread, which unloads them.
            })
            .expect("failed to spawn worker thread");

        (
            Worker {
                jobs: job_sender,
                thread: Some(thread),
            },
            ready_receiver,
        )
    }

    /// Queues a translation and waits for the result.
    pub async fn translate(
        &self,
        segments: Vec<String>,
        options: TranslateOptions,
    ) -> Result<Vec<String>> {
        let (reply, receiver) = oneshot::channel();
        self.jobs
            .send(Job {
                segments,
                options,
                reply,
            })
            .map_err(|_| Error::WorkerGone)?;
        receiver.await.map_err(|_| Error::WorkerGone)?
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        // Close the queue, then wait for the thread so the models are
        // really unloaded when the drop returns.
        drop(std::mem::replace(&mut self.jobs, mpsc::channel().0));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
