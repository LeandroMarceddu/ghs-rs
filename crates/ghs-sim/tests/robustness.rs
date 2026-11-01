//! Robustness tests: the simulator must never panic on controller input.

use std::time::Duration;

use ghs_sim::{SimProfile, Simulator};
use proptest::prelude::*;

#[test]
fn arbitrary_malformed_lines_do_not_panic() {
    let lines: &[&str] = &[
        "",
        " ",
        "G",
        "G1",
        "G1 X",
        "G1 X1..2",
        "G1 X1e3",
        "G999",
        "M999",
        "G2 X1",
        "G2 X1 Y1 I",
        "G4",
        "G10 L2 P99 X1",
        "G10 L2 X1",
        "G28 X",
        "G92 X",
        "T-1",
        "T1.5",
        "M6 T",
        "$",
        "$$$",
        "$100=",
        "$100=NaN",
        "$100=inf",
        "$9999",
        "$SED=x",
        "$J=",
        "$J=G92 X1 F100",
        "$J=G91 X1 F-1",
        "$N2=G0 X1",
        "$RST=?",
        "$H?",
        "$HELP nope",
        "G0 X1 Y2 Z3 A4 B5 C6 U7 V8 W9",
        "G0 X1e999",
        "N999999999999999999999 G0 X1",
    ];

    for line in lines {
        let mut sim = Simulator::new(SimProfile::GenericFull);
        let _ = sim.feed(line.as_bytes());
        let _ = sim.feed(b"\n");
        let _ = sim.tick(Duration::from_millis(10));
        let _ = sim.snapshot();
    }
}

#[test]
fn overlong_lines_are_rejected_without_panicking() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    let mut bytes = vec![b'G'; 4096];
    bytes.push(b'\n');
    let out = String::from_utf8(sim.feed(&bytes)).expect("ASCII output");
    assert!(out.contains("error:11"), "{out}");

    assert!(sim.feed(b"$I\n").ends_with(b"ok\r\n"));
}

#[test]
fn invalid_utf8_on_the_wire_is_rejected() {
    let mut sim = Simulator::new(SimProfile::FlexiHal);
    let out = sim.feed(&[0xFF, 0xFE, b'\n']);
    assert_eq!(String::from_utf8(out).expect("ASCII output"), "error:1\r\n");
    assert!(sim.feed(b"$I\n").ends_with(b"ok\r\n"));
}

#[test]
fn bannerless_startup_answers_status_requests() {
    let mut sim = Simulator::new_without_banner(SimProfile::GenericFull);
    assert!(!sim.banner_enabled());
    assert!(sim.feed(&[0x18]).is_empty());
    let status = String::from_utf8(sim.feed(b"?")).expect("status");
    assert!(status.starts_with("<Idle"));
}

#[test]
fn injected_alarms_always_terminate() {
    for code in [1, 2, 10, 11, 17, 20, 99, u16::MAX] {
        let mut sim = Simulator::new(SimProfile::FlexiHal);
        let out = sim.raise_alarm(code);
        assert_eq!(
            String::from_utf8(out).expect("ASCII"),
            format!("ALARM:{code}\r\n")
        );
        let _ = sim.feed(b"$X\n");
        let _ = sim.feed(b"$H\n");
        let _ = sim.feed(b"G0 X1 F100\n");
        let _ = sim.tick(Duration::from_secs(1));
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, ..ProptestConfig::default() })]

    #[test]
    fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {
        let mut sim = Simulator::new(SimProfile::GenericFull);
        let _ = sim.feed(&bytes);
        let _ = sim.tick(Duration::from_millis(50));
        let _ = sim.snapshot();
    }

    #[test]
    fn arbitrary_ascii_lines_never_panic(line in "[ -~]{0,240}") {
        let mut sim = Simulator::new(SimProfile::FlexiHal);
        let _ = sim.feed(line.as_bytes());
        let _ = sim.feed(b"\n");
        let _ = sim.feed(b"?");
        let _ = sim.feed(b"$#\n");
        let _ = sim.tick(Duration::from_millis(25));
        let _ = sim.snapshot();
    }
}
