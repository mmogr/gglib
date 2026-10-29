//! `tojson` as llama-server's Jinja engine writes it, which is Python's
//! `json.dumps` (`common/jinja/value.cpp:249-277, 1533-1690`): `", "` and
//! `": "` between items, nothing HTML-escaped, non-ASCII kept unless
//! `ensure_ascii`, and floats as Python's `repr`. minijinja's own `tojson`
//! writes compact JSON with `<`, `>`, `&` and `'` escaped.

use minijinja::value::{Rest, ValueKind};
use minijinja::{Error, ErrorKind, Value};

/// How the JSON is laid out.
struct Layout {
    /// Spaces per level; a negative indent writes everything on one line.
    indent: i64,
    item_sep: String,
    key_sep: String,
}

/// The `tojson` filter and function. Positional arguments come in the order
/// `ensure_ascii, indent, separators, sort_keys`, as llama reads them, and
/// a keyword argument wins over its position.
pub(super) fn tojson(value: &Value, Rest(args): Rest<Value>) -> Result<String, Error> {
    let (positional, named) = match args.split_last() {
        Some((last, rest)) if last.is_kwargs() => (rest, Some(last)),
        _ => (&args[..], None),
    };
    let arg = |name: &str, position: usize| -> Value {
        named
            .and_then(|kwargs| kwargs.get_item(&Value::from(name)).ok())
            .filter(|v| !v.is_undefined())
            .or_else(|| positional.get(position).cloned())
            .unwrap_or(Value::UNDEFINED)
    };
    let ensure_ascii = arg("ensure_ascii", 0).is_true();
    let indent = arg("indent", 1);
    let indent = if indent.is_integer() {
        i64::try_from(indent)?
    } else {
        -1
    };
    let separators = arg("separators", 2);
    if arg("sort_keys", 3).is_true() {
        return Err(Error::new(
            ErrorKind::InvalidOperation,
            "tojson sort_keys=true not implemented",
        ));
    }
    let separator = |i: usize| -> Option<String> {
        (separators.kind() == ValueKind::Seq)
            .then(|| separators.get_item_by_index(i).ok())
            .flatten()
            .filter(|s| !s.is_undefined())
            .map(|s| s.to_string())
    };
    let layout = Layout {
        indent,
        item_sep: separator(0).unwrap_or_else(|| (if indent < 0 { ", " } else { "," }).to_owned()),
        key_sep: separator(1).unwrap_or_else(|| ": ".to_owned()),
    };
    let mut out = String::new();
    write_value(&mut out, value, 0, &layout)?;
    Ok(if ensure_ascii { to_ascii(&out) } else { out })
}

fn write_value(
    out: &mut String,
    value: &Value,
    level: usize,
    layout: &Layout,
) -> Result<(), Error> {
    match value.kind() {
        ValueKind::Bool => out.push_str(if value.is_true() { "true" } else { "false" }),
        ValueKind::Number if value.is_integer() => out.push_str(&value.to_string()),
        ValueKind::Number => out.push_str(&float_json(f64::try_from(value.clone())?)),
        ValueKind::String => write_string(out, value.as_str().unwrap_or_default()),
        ValueKind::Seq | ValueKind::Iterable => {
            let items: Vec<Value> = value.try_iter()?.collect();
            write_container(out, ('[', ']'), &items, level, layout, |out, item| {
                write_value(out, item, level + 1, layout)
            })?;
        }
        ValueKind::Map => {
            let keys: Vec<Value> = value.try_iter()?.collect();
            write_container(out, ('{', '}'), &keys, level, layout, |out, key| {
                match key.as_str() {
                    Some(name) => write_string(out, name),
                    None if key.kind() == ValueKind::Number && !key.is_integer() => {
                        write_string(out, &float_json(f64::try_from(key.clone())?));
                    }
                    None => write_string(out, &key.to_string()),
                }
                out.push_str(&layout.key_sep);
                write_value(out, &value.get_item(key)?, level + 1, layout)
            })?;
        }
        _ => out.push_str("null"),
    }
    Ok(())
}

/// The items of an array or the pairs of an object between `open` and
/// `close`, one to a line when indented.
fn write_container(
    out: &mut String,
    (open, close): (char, char),
    items: &[Value],
    level: usize,
    layout: &Layout,
    mut write_item: impl FnMut(&mut String, &Value) -> Result<(), Error>,
) -> Result<(), Error> {
    let newline = if layout.indent >= 0 { "\n" } else { "" };
    let step = usize::try_from(layout.indent).unwrap_or(0);
    out.push(open);
    if !items.is_empty() {
        out.push_str(newline);
        for (i, item) in items.iter().enumerate() {
            out.push_str(&" ".repeat((level + 1) * step));
            write_item(out, item)?;
            if i + 1 < items.len() {
                out.push_str(&layout.item_sep);
            }
            out.push_str(newline);
        }
        out.push_str(&" ".repeat(level * step));
    }
    out.push(close);
    Ok(())
}

/// A JSON string: quotes, backslashes and control bytes escaped, nothing
/// else (`common/jinja/value.cpp:1605-1683`).
fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => push_unicode_escape(out, u32::from(c)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// `json` with every non-ASCII character written as `\uXXXX`, surrogate
/// pairs above the BMP, lower-case hex (`common/jinja/value.cpp:170-243`).
fn to_ascii(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    for c in json.chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            for unit in c.encode_utf16(&mut [0; 2]) {
                push_unicode_escape(&mut out, u32::from(*unit));
            }
        }
    }
    out
}

/// `\uXXXX` for a UTF-16 code unit, in lower-case hex.
fn push_unicode_escape(out: &mut String, unit: u32) {
    out.push_str("\\u");
    for shift in [12, 8, 4, 0] {
        out.extend(char::from_digit((unit >> shift) & 0xf, 16));
    }
}

/// A float as `json.dumps` writes it: `NaN` and `Infinity` as they are.
fn float_json(v: f64) -> String {
    if v.is_nan() {
        "NaN".to_owned()
    } else if v.is_infinite() {
        (if v < 0.0 { "-Infinity" } else { "Infinity" }).to_owned()
    } else {
        float_repr(v)
    }
}

/// Python's `repr` of a finite float: the shortest digits that read back,
/// fixed notation from 1e-4 up to 1e16, else `d.ddde±XX`
/// (`common/jinja/value.cpp:1533-1601`).
pub(super) fn float_repr(v: f64) -> String {
    // Rust's `{:e}` writes the shortest round-trip digits, as `d.ddde<exp>`.
    let scientific = format!("{:e}", v.abs());
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let exp10: i64 = exponent.parse().unwrap_or(0);
    let mut digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    while digits.len() > 1 && digits.ends_with('0') {
        digits.pop();
    }
    let n = i64::try_from(digits.len()).unwrap_or(i64::MAX);
    let point = exp10 + 1;
    let sign = if v.is_sign_negative() { "-" } else { "" };
    let zeros = |count: i64| "0".repeat(usize::try_from(count).unwrap_or(0));
    let body = if point > -4 && point <= 16 {
        if point <= 0 {
            format!("0.{}{digits}", zeros(-point))
        } else if point >= n {
            format!("{digits}{}.0", zeros(point - n))
        } else {
            let (whole, fraction) = digits.split_at(usize::try_from(point).unwrap_or(0));
            format!("{whole}.{fraction}")
        }
    } else {
        let (first, rest) = digits.split_at(1);
        let rest = if rest.is_empty() {
            String::new()
        } else {
            format!(".{rest}")
        };
        let sign = if exp10 < 0 { '-' } else { '+' };
        format!("{first}{rest}e{sign}{:02}", exp10.abs())
    };
    format!("{sign}{body}")
}
