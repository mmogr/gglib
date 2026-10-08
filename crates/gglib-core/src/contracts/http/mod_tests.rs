//! `path_segment`: a model identifier as one URL path segment. Where an
//! image is read by its id. And the event a command posts, among the routes
//! the daemon's suite sweeps.

use super::path_segment;

#[test]
fn an_identifier_is_one_segment_whatever_it_holds() {
    assert_eq!(path_segment("3"), "3");
    assert_eq!(path_segment("qwen3:coding"), "qwen3%3Acoding");
    assert_eq!(path_segment("org/name Q4"), "org%2Fname%20Q4");
    assert_eq!(path_segment("a?b#c%d"), "a%3Fb%23c%25d");
    assert_eq!(path_segment("modèle"), "mod%C3%A8le");
    assert_eq!(path_segment("v1.5-instruct_x~"), "v1.5-instruct_x~");
}

/// An image is read at its store's path with its id as the last segment,
/// on this machine and on the paired one, and both far routes are swept
/// with the other routes to that machine.
#[test]
fn an_image_is_read_at_its_stores_path_and_its_id() {
    use super::attachments::{attachment_path, remote_attachment_path};

    assert_eq!(attachment_path("abc"), "/api/attachments/abc");
    assert_eq!(remote_attachment_path("abc"), "/api/remote/attachments/abc");
    let swept = super::daemon::remote_route_contract();
    let id = "0".repeat(64);
    for route in [
        (&["POST"][..], "/api/remote/attachments".to_owned()),
        (&["GET"][..], format!("/api/remote/attachments/{id}")),
    ] {
        assert!(swept.contains(&route), "{route:?}");
    }
}

/// The event a command posts for a library change is swept with the routes
/// the CLI calls: for a route that takes it, for the daemon's token, and for
/// the refusal a page on another site gets.
#[test]
fn the_event_a_command_posts_is_swept_with_the_routes_the_cli_calls() {
    use super::daemon::{CLI_ROUTE_CONTRACT, EVENTS_PATH};

    assert!(CLI_ROUTE_CONTRACT.contains(&(&["POST"][..], EVENTS_PATH)));
}
