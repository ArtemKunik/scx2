//! Background thread that owns the HID device.

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

use crate::protocol::{self, Dump};
use crate::transport;
// ---------------------------------------------------------------------------
// background worker
// ---------------------------------------------------------------------------

pub(crate) enum Cmd {
    Refresh,
    Poll(bool),
    Write(Vec<Frame>),
}

/// A report to send, and whether it needs the execute frame afterwards.
pub(crate) type Frame = ([u8; protocol::REPORT_SIZE], bool);

pub(crate) enum Ev {
    Connected(Option<String>),
    Dump(Dump),
    Written(usize),
    Error(String),
}

pub(crate) fn spawn_worker(path: Option<String>, tx: Sender<Ev>, rx: Receiver<Cmd>) {
    // The device is opened inside the thread, so nothing has to be Send.
    thread::spawn(move || {
        let amp = match transport::Amp::open(path.as_deref()) {
            Ok(amp) => amp,
            Err(error) => {
                let _ = tx.send(Ev::Error(format!("{error:#}")));
                return;
            }
        };
        let banner = amp.handshake().ok().flatten();
        let _ = tx.send(Ev::Connected(banner));
        match amp.read_dump() {
            Ok(dump) => {
                let _ = tx.send(Ev::Dump(dump));
            }
            Err(error) => {
                let _ = tx.send(Ev::Error(format!("{error:#}")));
                return;
            }
        }

        let mut polling = false;
        loop {
            let command = if polling {
                rx.recv_timeout(Duration::from_millis(600))
            } else {
                rx.recv_timeout(Duration::from_millis(250))
            };
            match command {
                Ok(Cmd::Refresh) => match amp.read_dump() {
                    Ok(dump) => {
                        let _ = tx.send(Ev::Dump(dump));
                    }
                    Err(error) => {
                        let _ = tx.send(Ev::Error(format!("{error:#}")));
                        break;
                    }
                },
                Ok(Cmd::Poll(on)) => {
                    polling = on;
                    if on {
                        if let Ok(dump) = amp.read_dump() {
                            let _ = tx.send(Ev::Dump(dump));
                        }
                    }
                }
                Ok(Cmd::Write(frames)) => {
                    let mut sent = 0;
                    for (frame, execute) in frames {
                        if amp.write_report(&frame).is_err() {
                            break;
                        }
                        if execute {
                            // The amp buffers a block write until the execute frame.
                            let _ = amp.write_report(&protocol::execute_model_change());
                        }
                        sent += 1;
                        thread::sleep(Duration::from_millis(25));
                    }
                    let _ = tx.send(Ev::Written(sent));
                }
                Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {
                    if polling {
                        if let Ok(dump) = amp.read_dump() {
                            let _ = tx.send(Ev::Dump(dump));
                        }
                    }
                }
            }
        }
    });
}
