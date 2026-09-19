//! Shared connection engine for the terminal launcher and controller bridge.

mod backend;
#[cfg(windows)]
mod cache;
pub mod cli;
mod processing;
#[cfg(test)]
mod tests;
#[cfg(windows)]
mod vjoy;

use balance_board_protocol::{CalibratedSensors, LowPass2D};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Settings applied when starting a connection.
#[derive(Clone, Debug, Default)]
pub struct Config {
    /// Create a game controller. Monitoring alone needs no output driver.
    pub gamepad: bool,
    /// Skip stance centering.
    pub no_tare: bool,
    /// Disable smoothing.
    pub no_smooth: bool,
    /// Ignore the Windows per-device calibration cache.
    pub no_cache: bool,
}

/// Thread-safe controls; callers request actions without touching hardware.
#[derive(Default)]
pub struct Control {
    stop: AtomicBool,
    center: AtomicBool,
}

impl Control {
    /// Stop and release the board and virtual controller.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
    /// Capture a new centered stance without restarting the app.
    pub fn center(&self) {
        self.center.store(true, Ordering::Relaxed);
    }
    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }
}

/// Connection stage displayed by the application.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Discovering a board or waiting for it to reconnect.
    Waiting,
    /// Measuring a loaded, centered stance.
    Centering,
    /// Sensor data is available and any requested game output is active.
    Live,
    /// The connection loop has exited.
    Stopped,
}

/// Latest measurement and human-readable connection status.
#[derive(Clone, Debug)]
pub struct Status {
    /// Current connection stage.
    pub phase: Phase,
    /// Explanation or next action for the user.
    pub message: String,
    /// Total load in kilograms.
    pub total_kg: f32,
    /// Top right, bottom right, top left, bottom left, in kilograms.
    pub corners: [f32; 4],
    /// Centered and smoothed X/Y values in [-1, 1].
    pub lean: [f32; 2],
}

impl Status {
    fn message(phase: Phase, message: impl Into<String>) -> Self {
        Self {
            phase,
            message: message.into(),
            total_kg: 0.0,
            corners: [0.0; 4],
            lean: [0.0; 2],
        }
    }
}

pub(crate) struct Sample {
    pub weights: CalibratedSensors,
    pub button: bool,
}
pub(crate) trait Source {
    fn identity(&self) -> String;
    /// A bounded poll. None means no new sample yet.
    fn poll(&mut self) -> io::Result<Option<Sample>>;
}
pub(crate) trait Output {
    fn send(&mut self, frame: &processing::Processed) -> io::Result<()>;
    fn neutralize(&mut self) -> io::Result<()>;
}

/// Run until stopped. Status delivery should return promptly.
/// Stops cooperatively, resets output on loss of input, and reconnects to
/// the original device. Pairing is an explicit, separate user action.
pub fn run(config: Config, control: &Control, status: impl FnMut(Status)) -> io::Result<()> {
    let output = if config.gamepad {
        Some(backend::output()?)
    } else {
        None
    };
    run_engine(config, control, status, backend::open, output, pause)
}

fn run_engine(
    config: Config,
    control: &Control,
    mut status: impl FnMut(Status),
    mut open: impl FnMut(Option<&str>, bool) -> io::Result<Box<dyn Source>>,
    mut output: Option<Box<dyn Output>>,
    retry_wait: impl Fn(&Control),
) -> io::Result<()> {
    let mut identity = None;
    let result = (|| {
        status(Status::message(
            Phase::Waiting,
            "Looking for your board. Press Power if it is already paired. Retrying automatically; Ctrl+C to stop.",
        ));
        while !control.stopped() {
            let mut board = match open(identity.as_deref(), config.no_cache || identity.is_some()) {
                Ok(board) => board,
                Err(error) => {
                    status(Status::message(Phase::Waiting, error.to_string()));
                    retry_wait(control);
                    continue;
                }
            };
            identity = Some(board.identity());
            let mut tare = Tare::new(!config.no_tare);
            let mut filter = LowPass2D::new(if config.no_smooth { 1.0 } else { 0.4 });
            let mut last_sample = Instant::now();
            while !control.stopped() {
                if control.center.swap(false, Ordering::Relaxed) {
                    tare = Tare::new(true);
                    filter.reset();
                    if let Some(output) = output.as_mut() {
                        output.neutralize()?;
                    }
                }
                let polled = match board.poll() {
                    Ok(None) if last_sample.elapsed() < Duration::from_secs(3) => continue,
                    Ok(None) => Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "No sensor data. Wake the board with Power; reconnecting.",
                    )),
                    other => other,
                };
                let sample = match polled {
                    Ok(Some(sample)) => sample,
                    Ok(None) => unreachable!(),
                    Err(error) => {
                        if let Some(output) = output.as_mut() {
                            output.neutralize()?;
                        }
                        status(Status::message(Phase::Waiting, error.to_string()));
                        break;
                    }
                };
                last_sample = Instant::now();
                let was_centering = tare.remaining > 0;
                let ready = tare.observe(sample.weights);
                if ready && was_centering {
                    filter.reset();
                }
                let processed = processing::process_weights(
                    sample.weights,
                    sample.button,
                    tare.offset,
                    &mut filter,
                );
                if ready {
                    if let Some(output) = output.as_mut() {
                        output.send(&processed)?;
                    }
                }
                status(Status {
                    phase: if ready { Phase::Live } else { Phase::Centering },
                    message: if ready && !processed.cog_loaded {
                        "Connected — board unloaded."
                    } else if ready {
                        "Connected — live readings."
                    } else {
                        "Stand comfortably centered while your stance is measured."
                    }
                    .into(),
                    total_kg: processed.total_kg,
                    corners: [
                        sample.weights.top_right,
                        sample.weights.bottom_right,
                        sample.weights.top_left,
                        sample.weights.bottom_left,
                    ],
                    lean: if ready {
                        [processed.cog_x, processed.cog_y]
                    } else {
                        [0.0; 2]
                    },
                });
            }
            drop(board);
            if !control.stopped() {
                retry_wait(control);
            }
        }
        Ok(())
    })();
    // Attempt neutralization on every cooperative exit, including output errors.
    let cleanup = output
        .as_mut()
        .map(|out| out.neutralize())
        .unwrap_or(Ok(()));
    status(Status::message(Phase::Stopped, "Disconnected"));
    result.and(cleanup)
}

fn pause(control: &Control) {
    for _ in 0..20 {
        if control.stopped() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

struct Tare {
    remaining: usize,
    sum: (f32, f32),
    offset: (f32, f32),
}
impl Tare {
    fn new(enabled: bool) -> Self {
        Self {
            remaining: if enabled { 100 } else { 0 },
            sum: (0.0, 0.0),
            offset: (0.0, 0.0),
        }
    }
    fn observe(&mut self, weights: CalibratedSensors) -> bool {
        if self.remaining == 0 {
            return true;
        }
        if let Some(cog) = weights.center_of_gravity(2.0) {
            self.sum.0 += cog.x;
            self.sum.1 += cog.y;
            self.remaining -= 1;
            if self.remaining == 0 {
                self.offset = (self.sum.0 / 100.0, self.sum.1 / 100.0);
            }
        } else {
            // Stepping off mid-capture must not mix two different stances.
            *self = Self::new(true);
        }
        self.remaining == 0
    }
}
