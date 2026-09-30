//! What `FarChats` refuses before anything is sent, and what it never shows.

use gglib_core::domain::hub_chats::HubTurn;

use super::FarChats;
use crate::error::GuiError;

/// Nothing listens here: a request that got as far as sending would fail as
/// `Unavailable`, so a `ValidationFailed` proves it was never sent.
const NOWHERE: &str = "http://127.0.0.1:9/v1";

#[tokio::test]
async fn a_run_id_that_could_name_another_route_is_refused_before_sending() {
    let far = FarChats::new(NOWHERE, "sk-the-key").unwrap();
    let turn = HubTurn {
        conversation_id: 1,
        content: "hi".to_owned(),
    };
    for id in ["..", "a/b", "", "run.1", &"x".repeat(65)] {
        assert!(
            matches!(
                far.add_turn(id, &turn).await,
                Err(GuiError::ValidationFailed(_))
            ),
            "{id:?}"
        );
        assert!(
            matches!(far.cancel_run(id).await, Err(GuiError::ValidationFailed(_))),
            "{id:?}"
        );
        assert!(
            matches!(
                far.run_events(id, 0).await,
                Err(GuiError::ValidationFailed(_))
            ),
            "{id:?}"
        );
    }
}

#[test]
fn its_debug_form_never_prints_the_key() {
    let far = FarChats::new(NOWHERE, "sk-the-key").unwrap();
    let shown = format!("{far:?}");
    assert!(!shown.contains("sk-the-key"), "{shown}");
    assert!(shown.contains(NOWHERE), "{shown}");
}
