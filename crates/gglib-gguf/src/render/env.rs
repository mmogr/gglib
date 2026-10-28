//! The environment a chat template renders in: minijinja set to behave as
//! the pinned llama-server's own Jinja engine (`common/jinja/`) where a chat
//! template can tell the difference.

use std::fmt::Write as _;

use chrono::{DateTime, Local};
use minijinja::{AutoEscape, Environment, Error, ErrorKind, UndefinedBehavior};

use super::{RenderError, tojson};
use crate::template_env::base_env;

/// The name the one template is compiled under.
pub(super) const NAME: &str = "chat";

/// The longest `strftime_now` result llama's 100-byte buffer holds, less
/// its terminating NUL (`common/jinja/value.cpp:429-440`).
const STRFTIME_MAX: usize = 99;

/// `source` compiled in an environment whose `strftime_now` reads `now`.
///
/// Blocks are trimmed and left-stripped (`common/jinja/lexer.cpp:112-118`);
/// `\r\n` and `\r` read as `\n` (`lexer.cpp:43-51`), and one trailing
/// newline is dropped, minijinja's default as it is llama's
/// (`lexer.cpp:53-57`). llama looks for that newline in the source before
/// it reads `\r` as `\n`, so a source ending in a lone `\r` keeps a
/// trailing `\n` there and loses it here. An attribute of an undefined
/// value is undefined rather than an error
/// (`common/jinja/runtime.cpp:877-879`). Nothing is escaped.
pub(super) fn environment(
    source: &str,
    now: DateTime<Local>,
) -> Result<Environment<'static>, RenderError> {
    let mut env = base_env();
    env.set_undefined_behavior(UndefinedBehavior::Chainable);
    env.set_trim_blocks(true);
    env.set_lstrip_blocks(true);
    env.set_auto_escape_callback(|_| AutoEscape::None);
    env.add_filter("tojson", tojson::tojson);
    env.add_function("tojson", tojson::tojson);
    env.add_function("strftime_now", move |format: String| strftime(now, &format));
    env.add_template_owned(NAME, source.replace("\r\n", "\n").replace('\r', "\n"))?;
    Ok(env)
}

/// `now` in local time through `format`, refused where llama's `strftime`
/// fails: an empty result, or one longer than its 100-byte buffer holds.
/// A specifier outside the common set (`%Y`, `%m`, `%d`, `%b`, `%B`, ...)
/// is read as chrono reads it, not as libc does: chrono refuses `%Q`,
/// which libc writes as `Q`.
pub(super) fn strftime(now: DateTime<Local>, format: &str) -> Result<String, Error> {
    let failed = || {
        Error::new(
            ErrorKind::InvalidOperation,
            "strftime_now: failed to format time",
        )
    };
    let mut written = String::new();
    write!(written, "{}", now.format(format)).map_err(|_| failed())?;
    if written.is_empty() || written.len() > STRFTIME_MAX {
        return Err(failed());
    }
    Ok(written)
}
