//! Tests for the `--ctx-size` flag: its parse, and its resolution against a
//! model's own context length.

use crate::server_config::{CtxSizeArg, parse_ctx_size_flag};

#[test]
fn ctx_size_arg_parses_explicit_numeric() {
    assert_eq!(CtxSizeArg::parse("8192").unwrap(), CtxSizeArg::Value(8192));
}

#[test]
fn ctx_size_arg_parses_max_case_insensitive() {
    assert_eq!(CtxSizeArg::parse("max").unwrap(), CtxSizeArg::Max);
    assert_eq!(CtxSizeArg::parse("MAX").unwrap(), CtxSizeArg::Max);
    assert_eq!(CtxSizeArg::parse("  Max  ").unwrap(), CtxSizeArg::Max);
}

#[test]
fn ctx_size_arg_invalid_string_is_hard_error() {
    assert!(CtxSizeArg::parse("banana").is_err());
}

#[test]
fn ctx_size_arg_max_resolves_to_model_metadata() {
    assert_eq!(CtxSizeArg::Max.resolve(Some(131_072)), Some(131_072));
}

#[test]
fn ctx_size_arg_max_without_model_metadata_resolves_to_none() {
    assert_eq!(CtxSizeArg::Max.resolve(None), None);
}

#[test]
fn ctx_size_arg_value_ignores_model_metadata() {
    assert_eq!(CtxSizeArg::Value(4096).resolve(Some(131_072)), Some(4096));
}

#[test]
fn parse_ctx_size_flag_none_when_flag_omitted() {
    assert_eq!(parse_ctx_size_flag(None).unwrap(), None);
}

#[test]
fn parse_ctx_size_flag_propagates_parse_error() {
    assert!(parse_ctx_size_flag(Some("not-a-number")).is_err());
}
