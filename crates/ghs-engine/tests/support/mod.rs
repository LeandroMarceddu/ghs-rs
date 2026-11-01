//! Scripted transport used by the engine integration tests.
#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender, unbounded};
use ghs_serial::{Transport, TransportError, TransportEvent};

/// One item the engine wrote to the transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// A complete line, without its terminator.
    Line(String),
    /// A standalone real-time byte.
    Byte(u8),
}

impl Item {
    #[must_use]
    pub fn as_line(&self) -> Option<&str> {
        match self {
            Item::Line(line) => Some(line),
            Item::Byte(_) => None,
        }
    }

    #[must_use]
    pub fn as_byte(&self) -> Option<u8> {
        match self {
            Item::Byte(byte) => Some(*byte),
            Item::Line(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Expect {
    Line(String),
    Byte(u8),
}

#[derive(Debug, Clone)]
struct Exchange {
    expect: Expect,
    lines: Vec<String>,
}

impl Exchange {
    fn matches(&self, item: &Item) -> bool {
        match (&self.expect, item) {
            (Expect::Line(expected), Item::Line(actual)) => expected == actual,
            (Expect::Byte(expected), Item::Byte(actual)) => expected == actual,
            _ => false,
        }
    }
}

struct Inner {
    exchanges: VecDeque<Exchange>,
    status: Option<String>,
    full_status: Option<String>,
    respond_status: bool,
    sent: Vec<Vec<u8>>,
    unexpected: Vec<Item>,
    open: bool,
    closed_notified: bool,
}

/// The transport half of the scripted link.
pub struct ScriptedTransport {
    inner: Arc<Mutex<Inner>>,
    inbound: Receiver<TransportEvent>,
    outbound: Sender<TransportEvent>,
}

/// The test-side control half of the scripted link.
pub struct Control {
    inner: Arc<Mutex<Inner>>,
    outbound: Sender<TransportEvent>,
}

impl ScriptedTransport {
    #[must_use]
    pub fn new() -> (Self, Control) {
        let (outbound, inbound) = unbounded();
        let inner = Arc::new(Mutex::new(Inner {
            exchanges: VecDeque::new(),
            status: None,
            full_status: None,
            respond_status: true,
            sent: Vec::new(),
            unexpected: Vec::new(),
            open: true,
            closed_notified: false,
        }));
        (
            ScriptedTransport {
                inner: Arc::clone(&inner),
                inbound,
                outbound: outbound.clone(),
            },
            Control { inner, outbound },
        )
    }
}

impl Transport for ScriptedTransport {
    fn send(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        let mut inner = self.inner.lock().expect("script lock");
        if !inner.open {
            return Err(TransportError::Closed);
        }
        inner.sent.push(bytes.to_vec());

        let mut responses = Vec::new();
        for item in split_items(bytes) {
            let exchange = {
                let matches = inner
                    .exchanges
                    .front()
                    .is_some_and(|exchange| exchange.matches(&item));
                if matches {
                    inner.exchanges.pop_front()
                } else {
                    None
                }
            };

            if let Some(exchange) = exchange {
                for line in exchange.lines {
                    responses.push(format!("{line}\r\n").into_bytes());
                }
            } else if is_status_request(&item) {
                if inner.respond_status {
                    let line = match item {
                        Item::Byte(0x87) => {
                            inner.full_status.clone().or_else(|| inner.status.clone())
                        }
                        _ => inner.status.clone(),
                    };
                    if let Some(line) = line {
                        responses.push(format!("{line}\r\n").into_bytes());
                    }
                }
            } else {
                inner.unexpected.push(item);
            }
        }
        drop(inner);

        for response in responses {
            let _ = self.outbound.send(TransportEvent::Bytes(response));
        }
        Ok(())
    }

    fn events(&self) -> &Receiver<TransportEvent> {
        &self.inbound
    }

    fn close(&mut self) {
        let should_notify = {
            let mut inner = self.inner.lock().expect("script lock");
            if inner.open {
                inner.open = false;
                !inner.closed_notified
            } else {
                false
            }
        };
        if should_notify {
            self.inner.lock().expect("script lock").closed_notified = true;
            let _ = self.outbound.send(TransportEvent::Closed);
        }
    }

    fn is_open(&self) -> bool {
        self.inner.lock().expect("script lock").open
    }
}

impl Drop for ScriptedTransport {
    fn drop(&mut self) {
        self.close();
    }
}

impl Control {
    /// Queue one expected line and the lines the controller answers with.
    pub fn expect_line(&self, line: &str, responses: &[&str]) {
        self.inner
            .lock()
            .expect("script lock")
            .exchanges
            .push_back(Exchange {
                expect: Expect::Line(line.to_string()),
                lines: responses.iter().map(|line| (*line).to_string()).collect(),
            });
    }

    /// Queue one expected real-time byte and the controller's answer.
    pub fn expect_byte(&self, byte: u8, responses: &[&str]) {
        self.inner
            .lock()
            .expect("script lock")
            .exchanges
            .push_back(Exchange {
                expect: Expect::Byte(byte),
                lines: responses.iter().map(|line| (*line).to_string()).collect(),
            });
    }

    /// Set the status line returned for both normal and full status requests.
    pub fn set_status(&self, line: &str) {
        let mut inner = self.inner.lock().expect("script lock");
        inner.status = Some(line.to_string());
        inner.full_status = Some(line.to_string());
    }

    /// Set only the line returned for a full `0x87` report.
    pub fn set_full_status(&self, line: &str) {
        self.inner.lock().expect("script lock").full_status = Some(line.to_string());
    }

    /// Enable or disable responses to status requests.
    pub fn set_respond_status(&self, respond: bool) {
        self.inner.lock().expect("script lock").respond_status = respond;
    }

    /// Push one unsolicited controller line.
    pub fn push_line(&self, line: &str) {
        let _ = self
            .outbound
            .send(TransportEvent::Bytes(format!("{line}\r\n").into_bytes()));
    }

    /// Push arbitrary controller bytes.
    pub fn push_bytes(&self, bytes: &[u8]) {
        let _ = self.outbound.send(TransportEvent::Bytes(bytes.to_vec()));
    }

    /// Every chunk the engine wrote, in order.
    #[must_use]
    pub fn sent(&self) -> Vec<Vec<u8>> {
        self.inner.lock().expect("script lock").sent.clone()
    }

    /// Every item the engine wrote, with lines and real-time bytes separated.
    #[must_use]
    pub fn sent_items(&self) -> Vec<Item> {
        self.inner
            .lock()
            .expect("script lock")
            .sent
            .iter()
            .flat_map(|chunk| split_items(chunk))
            .collect()
    }

    /// Every complete line the engine wrote.
    #[must_use]
    pub fn sent_lines(&self) -> Vec<String> {
        self.sent_items()
            .into_iter()
            .filter_map(|item| item.as_line().map(str::to_string))
            .collect()
    }

    /// Every standalone real-time byte the engine wrote.
    #[must_use]
    pub fn sent_bytes(&self) -> Vec<u8> {
        self.sent_items()
            .into_iter()
            .filter_map(|item| item.as_byte())
            .collect()
    }

    /// Items that did not match the next expected exchange and were not status
    /// requests.
    #[must_use]
    pub fn unexpected(&self) -> Vec<Item> {
        self.inner.lock().expect("script lock").unexpected.clone()
    }

    /// Drop the send log.
    pub fn clear_sent(&self) {
        self.inner.lock().expect("script lock").sent.clear();
    }

    /// Returns `true` while the transport half is open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.inner.lock().expect("script lock").open
    }
}

fn is_status_request(item: &Item) -> bool {
    matches!(item, Item::Byte(0x87 | 0x80 | b'?'))
}

fn split_items(bytes: &[u8]) -> Vec<Item> {
    if bytes.contains(&b'\n') {
        let mut items = Vec::new();
        let mut start = 0;
        for (index, byte) in bytes.iter().enumerate() {
            if *byte == b'\n' {
                let mut line = &bytes[start..index];
                if line.ends_with(b"\r") {
                    line = &line[..line.len().saturating_sub(1)];
                }
                items.push(Item::Line(String::from_utf8_lossy(line).into_owned()));
                start = index + 1;
            }
        }
        for byte in &bytes[start..] {
            items.push(Item::Byte(*byte));
        }
        items
    } else {
        bytes.iter().map(|byte| Item::Byte(*byte)).collect()
    }
}

/// Idle status report used by the scripted controller.
pub const IDLE_STATUS: &str = "<Idle|MPos:0.000,0.000,0.000|WPos:0.000,0.000,0.000|WCO:0.000,0.000,0.000|Bf:15,1024|FS:0,0|Pn:|WCS:G54|Ov:100,100,100|A:|MPG:0|H:1|T:0|TLR:0>";

/// Queue the standard `$I+`/`$$`/`$#`/`$G` discovery script.
pub fn standard_script(control: &Control) {
    standard_script_with_options(
        control,
        "[OPT:VNMSL+2,64,1024,3,0]",
        "[NEWOPT:ENUMS,RT+,HOME,ES,EXPR,TC,SED]",
        IDLE_STATUS,
    );
}

/// Queue discovery with a custom `OPT` line.
pub fn standard_script_with_opt(control: &Control, opt: &str) {
    standard_script_with_options(
        control,
        opt,
        "[NEWOPT:ENUMS,RT+,HOME,ES,EXPR,TC,SED]",
        IDLE_STATUS,
    );
}

/// Queue discovery with custom option and status lines.
pub fn standard_script_with_options(control: &Control, opt: &str, newopt: &str, status: &str) {
    control.set_status(status);
    control.expect_line(
        "$I+",
        &[
            "[VER:1.1f.20240604:]",
            opt,
            "[AXS:3:XYZ]",
            newopt,
            "[FIRMWARE:grblHAL]",
            "[SIGNALS:HSDEP]",
            "[NVS STORAGE:*EEPROM]",
            "[FREE MEMORY:80K]",
            "[DRIVER:STM32F446]",
            "[DRIVER VERSION:240428]",
            "[BOARD:FlexiHAL_20240604_210624]",
            "[AUX IO:4,4,0,0]",
            "[PLUGIN:MODBUS v0.16]",
            "[SPINDLE:Huanyang v1]",
            "ok",
        ],
    );
    control.expect_line(
        "$$",
        &["$0=10", "$22=1", "$100=250.000", "$110=8000.000", "ok"],
    );
    control.expect_line(
        "$#",
        &[
            "[G54:10.000,20.000,30.000]",
            "[G55:0.000,0.000,0.000]",
            "[G28:0.000,0.000,0.000]",
            "[G30:0.000,0.000,0.000]",
            "[G92:0.000,0.000,0.000]",
            "[TLO:0.000,0.000,0.000]",
            "[TLR:0.000]",
            "ok",
        ],
    );
    control.expect_line("$G", &["[GC:G0 G54 G17 G21 G90 G94 M5 M9 T0 F0 S0]", "ok"]);
    if newopt.contains("ENUMS") {
        control.expect_line(
            "$EA",
            &["[ALARMCODE:1|Hard limit|Hard limit triggered.]", "ok"],
        );
        control.expect_line(
            "$EE",
            &["[ERRORCODE:20|Unsupported command|Invalid g-code.]", "ok"],
        );
        control.expect_line(
            "$ES",
            &[
                "[SETTING:0|0|Step pulse time|microseconds|5|####0|3|1000|0|N/A]",
                "[SETTING:100|1|X steps per mm|step/mm|6|#####0.000|1|100000|0|N/A]",
                "ok",
            ],
        );
        control.expect_line("$EG", &["[SETTINGGROUP:1|N/A|Axes]", "ok"]);
    }
}

/// Wait until a predicate is true or the timeout expires.
pub fn wait_until(mut predicate: impl FnMut() -> bool, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if predicate() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}
