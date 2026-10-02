//! `error_codes_scan` over one sample file (#1119): a write in each place it
//! reads, and one in each place it must not.

use super::error_codes_scan::{Write, scan};

/// Each writer the scan reads, and around them what it must not read: a
/// comment, a string, characters and odd quotes, each followed by a writer
/// that a misread would hide, test code between writers, a method of a
/// writer's name, and definitions. Relays name their `fn`, the innermost.
const SAMPLE: &str = r##"
fn f() {
    ErrorResponse::with_code(
        message,
        "t1",
        "by_with_code",
    );
    coded(StatusCode::OK, "by_coded", m);
    error(StatusCode::OK, "t2", "by_error", m);
    run_error("by_run_error", m);
    Self::upstream_error_frame(m, "t4", "by_frame");
    HttpError::Coded { status, code: "by_coded_variant", message };
    LlmStreamEvent::UpstreamError { message, error_type, code: "by_stream_variant".to_owned() };
    RunError { code: "by_run_error_struct".to_owned(), message };
    json!({"error": {"type": "t3", "code": "by_json"}});
    json!({"code": 501});
    let code = x.get("code").unwrap_or("by_fallback");
    let code = x.code();
    let (message, code) = match e {
        A => ("m", "by_arm"),
        B => { other() }
        C => (m, c.unwrap_or_else(|| "by_arm_fallback".to_owned())),
        D => (m, d),
    };
    ErrorResponse::with_code(message, kind, passed);
    client.error(a, "t", "by_a_method", d);
    // with_code(message, "t", "in_a_comment")
    /* coded(StatusCode::OK, "in_a_block_comment", m) */
    let s = "coded(StatusCode::OK, \"in_a_string\", m)";
    let chars = ['"', '\"'];
    coded(StatusCode::OK, "after_the_characters", m);
    let raw = r#"an odd quote: " "#;
    coded(StatusCode::OK, "after_a_raw_string", m);
    let escaped = "an odd escaped quote: \" ";
    match x {
        #[cfg(test)]
        A => coded(StatusCode::OK, "in_a_test_arm", m),
        B => coded(StatusCode::OK, "after_a_test_arm", m),
    }
}
#[cfg(test)]
fn helper(a: u8) -> u8 { a }
#[cfg(all(test, unix))]
fn gated() { coded(StatusCode::OK, "in_a_gated_test", m); }
#[cfg(test)]
impl<A, B> T<A, B> { fn t() { coded(StatusCode::OK, "in_a_test_impl", m); } }
fn outer() {
    fn coded(status: S, code: &str, pad: [u8; 4]) { error(status, "t5", code, m); }
    coded(StatusCode::OK, held, m);
}
trait Tr { fn decl(&self); }
const C: () = { coded(StatusCode::OK, held_outside, m); };
impl E {
    fn code(&self) -> &'static str {
        match self { Self::A => "by_code_fn" }
    }
}
enum Ev { UpstreamError { code: String } }
struct RunError { code: String }
#[cfg(test)]
mod tests {
    fn t() { coded(StatusCode::OK, "in_a_test", m); }
}
"##;

#[test]
fn the_scan_reads_each_place_a_code_is_written() {
    let writes: Vec<Write> = scan(SAMPLE).into_iter().map(|(w, _)| w).collect();
    let code = |c: &str, k: Option<&str>| Write::Code(c.to_owned(), k.map(str::to_owned));
    let relay = |func: &str, spelling: &str| Write::Relay(func.to_owned(), spelling.to_owned());
    let expected = vec![
        code("by_with_code", Some("t1")),
        code("by_coded", None),
        code("by_error", Some("t2")),
        code("by_run_error", None),
        code("by_frame", Some("t4")),
        code("by_coded_variant", None),
        code("by_stream_variant", None),
        code("by_run_error_struct", None),
        code("by_json", Some("t3")),
        code("by_fallback", None),
        relay("f", "x.code()"),
        code("by_arm", None),
        relay("f", "{other()}"),
        code("by_arm_fallback", None),
        relay("f", "d"),
        relay("f", "passed"),
        code("after_the_characters", None),
        code("after_a_raw_string", None),
        code("after_a_test_arm", None),
        relay("coded", "code"),
        relay("outer", "held"),
        relay("", "held_outside"),
        code("by_code_fn", None),
    ];
    assert_eq!(writes, expected);
}
