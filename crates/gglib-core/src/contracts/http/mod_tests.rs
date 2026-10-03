//! `path_segment`: a model identifier as one URL path segment.

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
