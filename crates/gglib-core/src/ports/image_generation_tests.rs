//! Sizes on the wire, and each refusal's code, status and next step.

use std::time::Duration;

use super::*;
use crate::domain::ImageFamily;

#[test]
fn a_size_reads_and_writes_as_width_x_height() {
    let size: ImageSize = "1024x768".parse().unwrap();
    assert_eq!(
        size,
        ImageSize {
            width: 1024,
            height: 768
        }
    );
    assert_eq!(size.to_string(), "1024x768");
    assert_eq!("512X512".parse::<ImageSize>().unwrap().width, 512);
    for bad in ["1024", "x", "axb", "1024x", "-1x5", ""] {
        let err = bad.parse::<ImageSize>().unwrap_err();
        assert!(err.to_string().contains("WIDTHxHEIGHT"), "{bad}: {err}");
    }
}

#[test]
fn a_new_request_is_one_image_with_every_choice_left_open() {
    let request = ImageRequest::new("a cat");
    assert_eq!(
        (request.model, request.size, request.n, request.seed),
        (None, None, 1, None)
    );
}

/// Every refusal answers the code and status `docs/error-codes.json`
/// publishes for it.
#[test]
fn each_refusal_has_its_code_and_status() {
    let rule = ImageFamily::Flux1.recipe().size;
    let cases: Vec<(ImageError, Option<&str>, u16)> = vec![
        (
            ImageError::NotAnImageModel {
                model: "qwen3".into(),
            },
            Some("not_an_image_model"),
            400,
        ),
        (
            ImageError::Unavailable {
                reason: "no image model".into(),
            },
            Some("drawing_unavailable"),
            400,
        ),
        (
            ImageError::Gate(GateError::Unavailable("cli".into())),
            Some("drawing_unavailable"),
            400,
        ),
        (
            ImageError::InvalidSize {
                width: 7,
                height: 7,
                rule,
            },
            Some("invalid_image_size"),
            400,
        ),
        (
            ImageError::Invalid {
                message: "empty".into(),
            },
            Some("invalid_request"),
            400,
        ),
        (
            ImageError::Gate(GateError::Stalled(Duration::from_mins(3))),
            Some("admission_timeout"),
            503,
        ),
        (
            ImageError::Failed {
                message: "boom".into(),
            },
            Some("image_generation_failed"),
            502,
        ),
        (
            ImageError::Stalled {
                after: Duration::from_mins(3),
            },
            Some("image_render_stalled"),
            504,
        ),
        (
            ImageError::DeadlineExceeded,
            Some("image_render_stalled"),
            504,
        ),
        (
            ImageError::Runtime(ModelRuntimeError::ImageRuntimeNotInstalled),
            None,
            503,
        ),
        (
            ImageError::Refused {
                status: 409,
                code: Some("conflict".into()),
                message: "taken".into(),
            },
            Some("conflict"),
            409,
        ),
    ];
    for (error, code, status) in cases {
        assert_eq!(error.code(), code, "{error:?}");
        assert_eq!(error.http_status(), status, "{error:?}");
    }
}

/// A refusal says what to do next, not only what went wrong.
#[test]
fn each_message_says_what_to_do_next() {
    let rule = ImageFamily::Flux1.recipe().size;
    let size = ImageError::InvalidSize {
        width: 7,
        height: 7,
        rule,
    };
    assert_eq!(
        size.to_string(),
        "7x7 is not a size this model draws: each side a multiple of 64 from 256 to 1536; \
         try 1024x1024"
    );
    let failed = ImageError::Failed {
        message: "generate_image returned no results".into(),
    };
    assert!(failed.to_string().ends_with("or retry"), "{failed}");
    let stalled = ImageError::Stalled {
        after: Duration::from_mins(3),
    };
    assert!(stalled.to_string().contains("180s"), "{stalled}");
    assert!(
        ImageError::NotAnImageModel {
            model: "qwen3".into()
        }
        .to_string()
        .contains("name an image model")
    );
}
