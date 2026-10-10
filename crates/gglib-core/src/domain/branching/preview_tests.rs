//! The line an option is shown by.

use super::{PreviewRow, preview};
use crate::domain::chat::MessageRole::{self, Assistant, Tool, User};

fn turn(rows: &[(MessageRole, &'static str, usize)]) -> Vec<PreviewRow<'static>> {
    rows.iter()
        .map(|&(role, text, images)| PreviewRow { role, text, images })
        .collect()
}

#[test]
fn a_question_is_shown_by_its_first_line_that_is_not_blank() {
    assert_eq!(
        preview(&turn(&[(User, "  \n\t First line  \nsecond", 0)])),
        "First line"
    );
}

#[test]
fn a_long_line_is_cut_at_eighty_characters_not_bytes() {
    let long = "ü".repeat(100);
    let leaked: &'static str = Box::leak(long.into_boxed_str());
    assert_eq!(
        preview(&turn(&[(User, leaked, 0)])),
        format!("{}…", "ü".repeat(80))
    );
    let exact: &'static str = Box::leak("é".repeat(80).into_boxed_str());
    assert_eq!(preview(&turn(&[(User, exact, 0)])), "é".repeat(80));
}

#[test]
fn a_question_of_images_alone_is_called_what_it_holds() {
    assert_eq!(preview(&turn(&[(User, "", 1)])), "An image");
    assert_eq!(preview(&turn(&[(User, " ", 3)])), "3 images");
}

#[test]
fn a_reply_is_shown_by_its_last_line_of_its_own_never_a_tool_s() {
    let reply = turn(&[
        (Assistant, "Let me look.", 0),
        (Tool, "{\"weather\":\"rain\"}", 0),
        (Assistant, "Bring an umbrella.", 0),
    ]);
    assert_eq!(preview(&reply), "Bring an umbrella.");
    let unwritten = turn(&[(Assistant, "", 0), (Tool, "{}", 0)]);
    assert_eq!(preview(&unwritten), "(no text)");
}
