//! The budget a request is measured against, and the measurement.
//!
//! Split out of [`super::truncation`], which is at its file budget. The
//! budget is in characters because the request is; a model's context is in
//! tokens, so a budget is always some token count taken at some
//! chars-per-token ratio, and [`ContextBudget`] keeps both numbers.
//!
//! The ratio is needed again for the one part of a request whose length says
//! nothing about its cost: an image. A screenshot is a megabyte of base64 and
//! a few thousand prompt tokens, so [`measured_len`] counts each image at
//! [`image_url_tokens`] times the budget's ratio, in place of its URL's
//! length.

use serde_json::Value;

use super::images::{image_url_tokens, request_image_urls};

/// A history-truncation budget: the characters a request may measure, and the
/// context, in tokens, those characters stand for. Their quotient is the
/// chars-per-token ratio in use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextBudget {
    /// The most characters the request may measure, by [`measured_len`].
    pub chars: usize,
    /// The context size, in tokens, that `chars` was derived from.
    pub tokens: usize,
}

impl ContextBudget {
    /// The characters `tokens` tokens are counted as, at this budget's
    /// ratio, rounded up.
    const fn chars_for(self, tokens: usize) -> usize {
        match self.tokens {
            0 => 0,
            context => tokens.saturating_mul(self.chars).div_ceil(context),
        }
    }
}

/// The length `body` is measured at under `budget`: its bytes once
/// serialized, with each image counted at its estimated tokens, at the
/// budget's chars-per-token ratio, in place of the length of its URL.
///
/// A request with no images measures exactly its serialized length.
pub(super) fn measured_len(body: &Value, budget: ContextBudget) -> usize {
    request_image_urls(body).fold(serialized_len(body), |len, url| {
        len.saturating_sub(url.len())
            .saturating_add(budget.chars_for(image_url_tokens(url)))
    })
}

/// Byte length of `body` once serialized, without allocating a copy of it.
///
/// The budget is denominated in wire bytes, and a [`Value`] has none until it
/// is serialized — but a 200 KB conversation does not need to be materialized
/// twice just to be measured.
fn serialized_len(body: &Value) -> usize {
    let mut counter = CountingWriter::default();
    // Serializing a `Value` cannot fail: it holds no non-string map keys and no
    // non-finite numbers, and the sink never errors. Reporting zero on that
    // unreachable branch degrades to "under budget", i.e. passthrough.
    if serde_json::to_writer(&mut counter, body).is_err() {
        return 0;
    }
    counter.0
}

/// An [`std::io::Write`] sink that keeps the byte count and discards the bytes.
#[derive(Default)]
struct CountingWriter(usize);

impl std::io::Write for CountingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0 += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "measure_tests.rs"]
mod measure_tests;
