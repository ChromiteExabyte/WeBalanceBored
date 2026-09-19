use super::*;
use std::{cell::RefCell, rc::Rc};

fn weights() -> CalibratedSensors {
    CalibratedSensors {
        top_right: 20.0,
        bottom_right: 20.0,
        top_left: 10.0,
        bottom_left: 10.0,
    }
}
struct FakeSource {
    fail: bool,
}
impl Source for FakeSource {
    fn identity(&self) -> String {
        "same-board".into()
    }
    fn poll(&mut self) -> io::Result<Option<Sample>> {
        if self.fail {
            Err(io::ErrorKind::BrokenPipe.into())
        } else {
            Ok(Some(Sample {
                weights: weights(),
                button: false,
            }))
        }
    }
}
struct FakeOutput {
    events: Rc<RefCell<Vec<&'static str>>>,
    fail: bool,
}
impl Output for FakeOutput {
    fn neutralize(&mut self) -> io::Result<()> {
        self.events.borrow_mut().push("neutral");
        Ok(())
    }
    fn send(&mut self, _: &processing::Processed) -> io::Result<()> {
        self.events.borrow_mut().push("send");
        if self.fail {
            Err(io::ErrorKind::BrokenPipe.into())
        } else {
            Ok(())
        }
    }
}

#[test]
fn reconnect_neutralizes_before_reopening_same_device_and_stop_cleans_up() {
    let control = Control::default();
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut attempts = 0;
    run_engine(
        Config {
            no_tare: true,
            ..Config::default()
        },
        &control,
        |status| {
            if status.phase == Phase::Live {
                control.stop();
            }
        },
        |identity, fresh| {
            attempts += 1;
            if attempts == 1 {
                assert_eq!(identity, None);
                assert!(!fresh);
            } else {
                assert_eq!(identity, Some("same-board"));
                assert!(fresh);
                assert_eq!(&*events.borrow(), &["open", "neutral"]);
            }
            events.borrow_mut().push("open");
            Ok(Box::new(FakeSource {
                fail: attempts == 1,
            }))
        },
        Some(Box::new(FakeOutput {
            events: events.clone(),
            fail: false,
        })),
        |_| {},
    )
    .unwrap();
    assert_eq!(attempts, 2);
    assert_eq!(
        &*events.borrow(),
        &["open", "neutral", "open", "send", "neutral"]
    );
}

#[test]
fn output_failure_is_fatal_and_still_clears_controller() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut attempts = 0;
    let error = run_engine(
        Config {
            no_tare: true,
            ..Config::default()
        },
        &Control::default(),
        |_| {},
        |_, _| {
            attempts += 1;
            Ok(Box::new(FakeSource { fail: false }))
        },
        Some(Box::new(FakeOutput {
            events: events.clone(),
            fail: true,
        })),
        |_| {},
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
    assert_eq!(attempts, 1);
    assert_eq!(&*events.borrow(), &["send", "neutral"]);
}

#[test]
fn stop_while_waiting_does_not_open_again() {
    let control = Control::default();
    let mut attempts = 0;
    run_engine(
        Config::default(),
        &control,
        |_| {},
        |_, _| {
            attempts += 1;
            Err(io::ErrorKind::NotFound.into())
        },
        None,
        |control| control.stop(),
    )
    .unwrap();
    assert_eq!(attempts, 1);
}

#[test]
fn monitoring_does_not_require_an_output_device() {
    let control = Control::default();
    run_engine(
        Config {
            no_tare: true,
            ..Config::default()
        },
        &control,
        |status| {
            if status.phase == Phase::Live {
                assert_eq!(status.total_kg, 60.0);
                control.stop();
            }
        },
        |_, _| Ok(Box::new(FakeSource { fail: false })),
        None,
        |_| {},
    )
    .unwrap();
}

#[test]
fn stepping_off_resets_partial_tare() {
    let mut tare = Tare::new(true);
    for _ in 0..50 {
        assert!(!tare.observe(weights()));
    }
    assert!(!tare.observe(CalibratedSensors {
        top_right: 0.0,
        bottom_right: 0.0,
        top_left: 0.0,
        bottom_left: 0.0
    }));
    for _ in 0..99 {
        assert!(!tare.observe(weights()));
    }
    assert!(tare.observe(weights()));
    assert!((tare.offset.0 - 1.0 / 3.0).abs() < 1e-5);
}

#[test]
fn completed_tare_does_not_emit_old_smoothing_offset() {
    let control = Control::default();
    run_engine(
        Config::default(),
        &control,
        |status| {
            if status.phase == Phase::Live {
                assert!(status.lean[0].abs() < 1e-5);
                control.stop();
            }
        },
        |_, _| Ok(Box::new(FakeSource { fail: false })),
        None,
        |_| {},
    )
    .unwrap();
}
