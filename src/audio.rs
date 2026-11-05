//! Helpers shared by the tuner and the recorder.
//!
//! Both own a thread that holds a cpal stream, and both replace that stream
//! when the person picks another input. Stopping has to be bounded: opening a
//! stream can block inside the platform audio backend — PipeWire's ALSA plugin
//! does that while it waits for a node — and a thread stuck there must not
//! freeze the editor or keep the process alive.

use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How long a stop waits for the audio thread to release the device.
pub(crate) const RELEASE_TIMEOUT: Duration = Duration::from_millis(750);

/// Join `worker`, or leave it detached when it outlives `timeout`.
///
/// A detached worker keeps the device until the process exits, which is the
/// price of not blocking the caller. It cannot outlive the app's own state:
/// every channel it holds is already dropped by the time this returns.
pub(crate) fn join_bounded(worker: JoinHandle<()>, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while !worker.is_finished() {
        if Instant::now() >= deadline {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let _ = worker.join();
}
