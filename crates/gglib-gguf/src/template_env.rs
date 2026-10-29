//! The minijinja environment every chat template render in this crate starts
//! from: Python's string methods, which HF templates call freely, and
//! `raise_exception`, through which a template refuses an input. The template
//! probe and [`crate::render`] each lay their own undefined behaviour, clock
//! and filters over it.

use minijinja::{Environment, Error, ErrorKind, Value};

/// A fresh environment with the Python string methods and `raise_exception`.
pub(crate) fn base_env<'source>() -> Environment<'source> {
    let mut env = Environment::new();
    // Python-isms (`.strip()`, `.split()`, …) that HF templates use freely.
    env.set_unknown_method_callback(minijinja_contrib::pycompat::unknown_method_callback);
    // HF templates abort unsupported inputs through `raise_exception`.
    env.add_function("raise_exception", |msg: String| -> Result<Value, Error> {
        Err(Error::new(ErrorKind::InvalidOperation, msg))
    });
    env
}
