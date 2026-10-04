//! An attachment's id, and the shapes a client reads.

use serde_json::json;

use super::*;

/// SHA-256 of the three bytes `abc`, from FIPS 180-2.
const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

#[test]
fn an_id_is_the_sha256_of_the_bytes_in_lowercase_hex() {
    assert_eq!(AttachmentId::of(b"abc").as_str(), ABC);
    assert_eq!(AttachmentId::of(b"abc"), AttachmentId::parse(ABC).unwrap());
    assert_ne!(AttachmentId::of(b"abd").as_str(), ABC);
}

#[test]
fn only_64_lowercase_hex_characters_are_an_id() {
    assert!(AttachmentId::parse(ABC).is_ok());
    for bad in [
        "",
        &ABC[1..],
        &format!("{ABC}0"),
        &ABC.to_uppercase(),
        &ABC.replace('b', "g"),
        &ABC.replace('b', "/"),
    ] {
        assert_eq!(AttachmentId::parse(bad), Err(InvalidAttachmentId), "{bad}");
    }
}

#[test]
fn an_id_is_a_json_string_and_a_bad_one_does_not_deserialise() {
    let id = AttachmentId::parse(ABC).unwrap();
    assert_eq!(serde_json::to_value(&id).unwrap(), json!(ABC));
    assert_eq!(
        serde_json::from_value::<AttachmentId>(json!(ABC)).unwrap(),
        id
    );
    assert!(serde_json::from_value::<AttachmentId>(json!("../etc/passwd")).is_err());
    assert_eq!(id.to_string(), ABC);
}

#[test]
fn an_upload_answers_the_info_and_the_tokens_as_one_object() {
    let upload = AttachmentUpload {
        info: AttachmentInfo {
            id: AttachmentId::parse(ABC).unwrap(),
            mime: "image/png".into(),
            width: 2560,
            height: 1440,
        },
        image_tokens: 3600,
    };
    assert_eq!(
        serde_json::to_value(&upload).unwrap(),
        json!({
            "id": ABC,
            "mime": "image/png",
            "width": 2560,
            "height": 1440,
            "image_tokens": 3600,
        })
    );
}

#[test]
fn a_blob_is_debug_printed_as_its_length_and_never_its_bytes() {
    let blob = AttachmentBlob {
        mime: "image/png".into(),
        data: b"SECRETPIXELS".to_vec(),
    };
    let printed = format!("{blob:?}");
    assert!(printed.contains("bytes: 12"), "{printed}");
    assert!(!printed.contains("SECRET"), "{printed}");
    assert!(!printed.contains("83"), "{printed}");
}

/// A model is sent an image as a base64 data URL of its stored type.
#[test]
fn a_blob_is_sent_as_a_base64_data_url_of_its_type() {
    let blob = AttachmentBlob {
        mime: "image/jpeg".into(),
        data: vec![0xFF, 0xD8, 0xFF, 0x00],
    };
    assert_eq!(blob.data_url(), "data:image/jpeg;base64,/9j/AA==");
}
