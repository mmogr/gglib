//! Each error gglib answers maps onto the case an app words.

use super::*;

const REPO: &str = "owner/model-GGUF";

fn network(status: u16) -> DownloadError {
    DownloadError::network_with_status("download failed", status)
}

#[test]
fn the_hubs_refusals_map_case_for_case() {
    let model_id = REPO.to_owned();
    let cases = [
        (
            HfPortError::AuthRequired {
                model_id: model_id.clone(),
            },
            GglibError::AuthRequired,
        ),
        (
            HfPortError::ModelNotFound {
                model_id: model_id.clone(),
            },
            GglibError::NotFound,
        ),
        (
            HfPortError::QuantizationNotFound {
                model_id: model_id.clone(),
                quantization: "Q4_K_M".to_owned(),
            },
            GglibError::NotFound,
        ),
        (
            HfPortError::FileNotFound {
                model_id,
                path: "x.gguf".to_owned(),
            },
            GglibError::NotFound,
        ),
        (HfPortError::RateLimited, GglibError::RateLimited),
    ];
    for (hub, expected) in cases {
        assert_eq!(GglibError::from(hub), expected);
    }
}

#[test]
fn a_hub_that_cannot_be_reached_or_read_is_a_network_error_in_its_words() {
    let unreachable = HfPortError::Network {
        message: "dns error".to_owned(),
    };
    let unreadable = HfPortError::InvalidResponse {
        message: "not JSON".to_owned(),
    };

    assert_eq!(
        GglibError::from(unreachable),
        GglibError::Network {
            message: "dns error".to_owned()
        }
    );
    assert_eq!(
        GglibError::from(unreadable),
        GglibError::Network {
            message: "not JSON".to_owned()
        }
    );
}

/// The download manager keeps a transfer's HTTP status, and the status says
/// which case it is.
#[test]
fn a_downloads_http_status_says_which_case_it_is() {
    assert_eq!(GglibError::from(network(401)), GglibError::AuthRequired);
    assert_eq!(GglibError::from(network(403)), GglibError::AuthRequired);
    assert_eq!(GglibError::from(network(404)), GglibError::NotFound);
    assert_eq!(GglibError::from(network(429)), GglibError::RateLimited);
    assert_eq!(
        GglibError::from(network(503)),
        GglibError::Network {
            message: "download failed".to_owned()
        }
    );
}

#[test]
fn a_download_with_nothing_to_fetch_is_not_found() {
    assert_eq!(
        GglibError::from(DownloadError::not_found("x.gguf")),
        GglibError::NotFound
    );
}

/// A full disk reaches the download manager as an I/O error turned into
/// text; the OS's own words for it are what is read.
#[test]
fn a_write_to_a_full_disk_is_no_space() {
    let full = std::io::Error::from_raw_os_error(DISK_FULL);
    let failed_write = DownloadError::io("write", full.to_string());

    assert_eq!(GglibError::from(failed_write), GglibError::NoSpace);
    assert_eq!(GglibError::from_io(&full), GglibError::NoSpace);
}

#[test]
fn any_other_failed_write_is_internal_in_its_words() {
    let denied = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
    let failed_write = DownloadError::io("write", denied.to_string());

    let mapped = GglibError::from(failed_write);

    assert!(
        matches!(&mapped, GglibError::Internal { message } if message.contains(&denied.to_string())),
        "{mapped:?}"
    );
}

#[test]
fn a_model_the_library_does_not_hold_is_not_found() {
    assert_eq!(
        GglibError::from(RepositoryError::NotFound("model 7".to_owned())),
        GglibError::NotFound
    );
    assert!(matches!(
        GglibError::from(RepositoryError::Storage("locked".to_owned())),
        GglibError::Internal { .. }
    ));
}
