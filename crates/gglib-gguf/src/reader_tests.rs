//! Tests for [`super`]: the primitives it reads, and the malformed files it
//! refuses without ending the process.

use std::collections::HashMap;

use gglib_core::{GgufParseError, GgufParserPort};

use super::*;
use crate::{GgufParser, write_string_gguf};

const U8: u32 = 0;
const U32: u32 = 4;
const I32: u32 = 5;
const STRING: u32 = 8;
const ARRAY: u32 = 9;

/// Where a file holds its metadata count: after the magic, the version and
/// the tensor count.
const METADATA_COUNT_AT: usize = 16;

/// A reader over `bytes`, as over a file that holds exactly them.
fn reader(bytes: &[u8]) -> GgufReader<&[u8]> {
    GgufReader {
        reader: bytes,
        remaining: bytes.len() as u64,
    }
}

/// A reader over `bytes` that takes its file to go on without end, so that
/// only what the machine can hold is left to refuse a size.
fn endless(bytes: &[u8]) -> GgufReader<&[u8]> {
    GgufReader {
        reader: bytes,
        remaining: u64::MAX,
    }
}

/// The bytes of a well-formed file that holds `pairs` as string metadata.
fn file_of(pairs: &[(&str, &str)]) -> Vec<u8> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("model.gguf");
    write_string_gguf(&path, pairs);
    std::fs::read(path).unwrap()
}

/// What the parser reads from a file that holds `bytes`.
fn parse(bytes: &[u8]) -> Result<HashMap<String, String>, GgufParseError> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("model.gguf");
    std::fs::write(&path, bytes).unwrap();
    GgufParser::new().parse(&path).map(|parsed| parsed.metadata)
}

/// What the parser says of a file that holds `bytes`, in refusing it as
/// malformed.
fn refusal(bytes: &[u8]) -> String {
    match parse(bytes) {
        Err(GgufParseError::InvalidFormat(message)) => message,
        other => panic!("the file is not refused as malformed: {other:?}"),
    }
}

fn put_u64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

/// A string as a file holds it: its length, then its bytes.
fn string(text: &str) -> Vec<u8> {
    [&(text.len() as u64).to_le_bytes(), text.as_bytes()].concat()
}

/// An array as a file holds it: its element type, the count it declares,
/// then `elements`.
fn array(element_type: u32, count: u64, elements: &[u8]) -> Vec<u8> {
    [
        &element_type.to_le_bytes()[..],
        &count.to_le_bytes(),
        elements,
    ]
    .concat()
}

/// Arrays nested `depth` deep as a file holds them: each the one element of
/// the last, and the innermost empty.
fn nested(depth: usize) -> Vec<u8> {
    let mut value = array(ARRAY, 1, &[]).repeat(depth - 1);
    value.extend(array(U8, 0, &[]));
    value
}

/// The file of `bytes` with one more metadata pair: `key`, and `value` as
/// the bytes of a value of `value_type`.
fn with_pair(mut bytes: Vec<u8>, key: &str, value_type: u32, value: &[u8]) -> Vec<u8> {
    let count = u64::from_le_bytes(bytes[METADATA_COUNT_AT..][..8].try_into().unwrap());
    put_u64(&mut bytes, METADATA_COUNT_AT, count + 1);
    bytes.extend(string(key));
    bytes.extend_from_slice(&value_type.to_le_bytes());
    bytes.extend_from_slice(value);
    bytes
}

/// A well-formed file with a value of each shape: strings, a number, an
/// array of strings and an array of numbers.
fn sample() -> Vec<u8> {
    let bytes = file_of(&[
        ("general.architecture", "llama"),
        ("general.name", "Qwen3 8B 多语言"),
    ]);
    let tokens = [string("<s>"), string("</s>")].concat();
    let bytes = with_pair(
        bytes,
        "tokenizer.ggml.tokens",
        ARRAY,
        &array(STRING, 2, &tokens),
    );
    let bytes = with_pair(bytes, "llama.context_length", U32, &4096_u32.to_le_bytes());
    let types = [1_i32, 2, 3].map(i32::to_le_bytes).concat();
    with_pair(
        bytes,
        "tokenizer.ggml.token_type",
        ARRAY,
        &array(I32, 3, &types),
    )
}

#[test]
fn test_read_u32() {
    let data = [0x01, 0x02, 0x03, 0x04];
    let mut reader = reader(&data);
    assert_eq!(reader.read_u32().unwrap(), 0x0403_0201);
}

#[test]
fn test_read_string() {
    // Length (u64 LE) = 5, then "hello"
    let data = [
        0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'h', b'e', b'l', b'l', b'o',
    ];
    let mut reader = reader(&data);
    assert_eq!(reader.read_string().unwrap(), "hello");
}

#[test]
fn test_read_magic_valid() {
    let mut reader = reader(&GGUF_MAGIC);
    assert!(reader.read_magic().is_ok());
}

#[test]
fn test_read_magic_invalid() {
    let mut reader = reader(&[0x00, 0x00, 0x00, 0x00]);
    assert!(matches!(
        reader.read_magic(),
        Err(GgufInternalError::InvalidMagic)
    ));
}

#[test]
fn test_read_version_valid() {
    let data = [0x02, 0x00, 0x00, 0x00]; // version 2
    let mut reader = reader(&data);
    assert_eq!(reader.read_version().unwrap(), 2);
}

#[test]
fn test_read_version_invalid() {
    let data = [0x05, 0x00, 0x00, 0x00]; // version 5 - unsupported
    let mut reader = reader(&data);
    assert!(matches!(
        reader.read_version(),
        Err(GgufInternalError::UnsupportedVersion(5))
    ));
}

#[test]
fn test_read_value_u32() {
    let data = [0x2A, 0x00, 0x00, 0x00]; // 42
    let mut reader = reader(&data);
    let value = reader.read_value(4).unwrap();
    assert!(matches!(value, GgufValue::U32(42)));
}

#[test]
fn test_read_value_bool() {
    let data = [0x01];
    let mut reader = reader(&data);
    let value = reader.read_value(7).unwrap();
    assert!(matches!(value, GgufValue::Bool(true)));
}

#[test]
fn a_well_formed_file_parses_to_what_it_holds() {
    let expected = [
        ("general.architecture", "llama"),
        ("general.name", "Qwen3 8B 多语言"),
        ("tokenizer.ggml.tokens", "[<s>, </s>]"),
        ("llama.context_length", "4096"),
        ("tokenizer.ggml.token_type", "[1, 2, 3]"),
    ];

    let parsed = parse(&sample()).unwrap();

    let expected = expected.map(|(key, value)| (key.to_owned(), value.to_owned()));
    assert_eq!(parsed, HashMap::from(expected));
}

/// The value's length is one a real malformed file declared.
#[test]
fn a_string_longer_than_the_bytes_left_is_refused_by_name() {
    let mut value = file_of(&[("general.name", "Qwen")]);
    let length_at = value.len() - "Qwen".len() - 8;
    put_u64(&mut value, length_at, 7_954_895_644_034_859_008);
    let mut key = file_of(&[("general.name", "Qwen")]);
    put_u64(&mut key, METADATA_COUNT_AT + 8, u64::MAX);

    assert_eq!(
        refusal(&value),
        "string length 7954895644034859008 is more than the 4 bytes left can hold"
    );
    assert_eq!(
        refusal(&key),
        "string length 18446744073709551615 is more than the 28 bytes left can hold"
    );
}

#[test]
fn a_string_may_fill_the_bytes_left_and_not_one_more() {
    let mut bytes = file_of(&[("general.name", "Qwen")]);
    assert_eq!(parse(&bytes).unwrap()["general.name"], "Qwen");

    let length_at = bytes.len() - "Qwen".len() - 8;
    put_u64(&mut bytes, length_at, 5);

    assert_eq!(
        refusal(&bytes),
        "string length 5 is more than the 4 bytes left can hold"
    );
}

#[test]
fn an_array_count_no_file_could_hold_is_refused_by_name() {
    let bytes = file_of(&[("general.architecture", "llama")]);
    let tokens = array(STRING, 1 << 50, &[]);
    let bytes = with_pair(bytes, "tokenizer.ggml.tokens", ARRAY, &tokens);

    assert_eq!(
        refusal(&bytes),
        "array count 1125899906842624 is more than the 0 bytes left can hold"
    );
}

/// Each row is an element type, the fewest bytes one value of it takes, and
/// what an array of three zeroed values prints as.
#[test]
fn an_array_may_fill_the_bytes_left_with_its_elements_and_not_one_more() {
    let numbers = "[0, 0, 0]";
    let cases = [
        (0, 1, numbers),
        (1, 1, numbers),
        (2, 2, numbers),
        (3, 2, numbers),
        (4, 4, numbers),
        (5, 4, numbers),
        (6, 4, numbers),
        (7, 1, "[false, false, false]"),
        (8, 8, "[, , ]"),
        (9, 12, "[[], [], []]"),
        (10, 8, numbers),
        (11, 8, numbers),
        (12, 8, numbers),
    ];
    for (element_type, width, printed) in cases {
        let elements = vec![0; 3 * width];
        let whole = with_pair(file_of(&[]), "k", ARRAY, &array(element_type, 3, &elements));
        assert_eq!(parse(&whole).unwrap()["k"], printed, "type {element_type}");

        let short = &whole[..whole.len() - 1];

        let left = 3 * width - 1;
        assert_eq!(
            refusal(short),
            format!("array count 3 is more than the {left} bytes left can hold"),
            "type {element_type}"
        );
    }
}

#[test]
fn an_array_of_a_type_the_format_lacks_is_still_refused_for_its_type() {
    let bytes = with_pair(file_of(&[]), "k", ARRAY, &array(13, 1, &[0]));

    assert_eq!(refusal(&bytes), "Unknown value type: 13");
}

#[test]
fn a_metadata_count_no_file_could_hold_is_refused_by_name() {
    let mut bytes = file_of(&[]);
    put_u64(&mut bytes, METADATA_COUNT_AT, u64::MAX);

    assert_eq!(
        refusal(&bytes),
        "metadata count 18446744073709551615 is more than the 0 bytes left can hold"
    );
}

/// The smallest pair is an empty key and a one-byte value, and the second
/// of two with one key is the one the parser keeps.
#[test]
fn the_metadata_may_fill_the_bytes_left_with_its_pairs_and_not_one_more() {
    let whole = with_pair(with_pair(file_of(&[]), "", U8, &[7]), "", U8, &[8]);
    let kept = HashMap::from([(String::new(), "8".to_owned())]);
    assert_eq!(parse(&whole).unwrap(), kept);

    let short = &whole[..whole.len() - 1];

    assert_eq!(
        refusal(short),
        "metadata count 2 is more than the 25 bytes left can hold"
    );
}

#[test]
fn arrays_may_nest_as_deep_as_the_limit_and_not_one_deeper() {
    let deepest = with_pair(file_of(&[]), "k", ARRAY, &nested(MAX_ARRAY_DEPTH));
    let printed = "[".repeat(MAX_ARRAY_DEPTH) + &"]".repeat(MAX_ARRAY_DEPTH);
    assert_eq!(parse(&deepest).unwrap()["k"], printed);

    let deeper = with_pair(file_of(&[]), "k", ARRAY, &nested(MAX_ARRAY_DEPTH + 1));

    assert_eq!(refusal(&deeper), "arrays nested more than 32 deep");
}

#[test]
fn a_string_the_file_could_hold_and_the_machine_cannot_is_refused_not_fatal() {
    let length = (1_u64 << 62).to_le_bytes();

    let refused = endless(&length).read_string().unwrap_err();

    assert_eq!(
        refused.to_string(),
        "I/O error: no memory for string length 4611686018427387904"
    );
}

/// The file was two bytes long when it was opened, and has grown since.
#[test]
fn a_file_that_grew_has_nothing_left_past_the_length_it_had() {
    let bytes = [&7_u32.to_le_bytes()[..], &string("grown")].concat();
    let mut reader = GgufReader {
        reader: &bytes[..],
        remaining: 2,
    };
    assert_eq!(reader.read_u32().unwrap(), 7);

    let refused = reader.read_string().unwrap_err();

    assert_eq!(
        refused.to_string(),
        "Invalid GGUF file: string length 5 is more than the 0 bytes left can hold"
    );
}

/// A file that shrank after it was opened holds less than the length it
/// had allows for.
#[test]
fn a_string_its_source_cuts_short_is_an_error() {
    let cut = [&5_u64.to_le_bytes()[..], b"he"].concat();

    let refused = endless(&cut).read_string().unwrap_err();

    assert!(
        matches!(&refused, GgufInternalError::Io(e) if e.kind() == io::ErrorKind::UnexpectedEof),
        "{refused}"
    );
}

#[test]
fn an_array_the_file_could_hold_and_the_machine_cannot_is_refused_not_fatal() {
    let value = array(U8, 1 << 62, &[]);

    let refused = endless(&value).read_value(ARRAY).unwrap_err();

    assert_eq!(
        refused.to_string(),
        "I/O error: no memory for array count 4611686018427387904"
    );
}

#[test]
fn only_the_whole_of_a_file_parses_and_no_prefix_of_it_is_fatal() {
    let whole = sample();
    assert!(parse(&whole).is_ok());

    for len in 0..whole.len() {
        let parsed = parse(&whole[..len]);

        assert!(parsed.is_err(), "the first {len} bytes parse: {parsed:?}");
    }
}

/// A length or a count with `0xFF` in one of its upper bytes is a size no
/// file holds. The only bytes that can be `0xFF` in a file that parses are
/// those nothing is sized, typed or spelled by: the eight of the tensor
/// count, and the four of each of the sample's numbers.
#[test]
fn no_one_corrupt_byte_in_a_file_is_fatal() {
    let whole = sample();

    let parsed = (0..whole.len()).filter(|&at| {
        let mut corrupt = whole.clone();
        corrupt[at] = 0xFF;
        parse(&corrupt).is_ok()
    });

    assert_eq!(parsed.count(), 8 + 4 * 4);
}
