//! Property tests: malformed controller input must never panic.

use ghs_proto::{
    LineParser, Response, ResponseParser, Streamer, StreamerConfig, parse_controller_info,
    parse_offset_line, parse_parser_state, parse_setting_line, parse_status, sanitize_line,
    validate_line,
};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn arbitrary_bytes_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..4096)) {
        let mut parser = ResponseParser::new();
        let _ = parser.push(&bytes);

        let mut lines = LineParser::new();
        let _ = lines.push(&bytes);

        let mut streamer = Streamer::new(StreamerConfig {
            rx_buffer_size: 64,
            max_line_len: 512,
            use_bf_topup: true,
        });
        let _ = streamer.enqueue(&String::from_utf8_lossy(&bytes));
        let _ = streamer.poll_output();
    }

    #[test]
    fn arbitrary_text_never_panics(text in "(?s).{0,512}") {
        let _ = Response::parse(&text);
        let _ = parse_status(&text);
        let _ = parse_offset_line(&text);
        let _ = parse_setting_line(&text);
        let _ = parse_parser_state(&text);
        let _ = parse_controller_info(&[&text]);
        let _ = sanitize_line(&text);
        let _ = validate_line(&text);
    }

    #[test]
    fn arbitrary_status_fields_never_panic(
        key in "[A-Za-z$]{1,8}",
        value in "(?s)[^>|]{0,64}"
    ) {
        let line = format!("<Idle|{key}:{value}|MPos:1,2,3>");
        if let Ok(report) = parse_status(&line) {
            let _ = report.machine_position();
            let _ = report.is_mpg_controlled();
            let _ = report.is_check_mode();
        }
    }
}
