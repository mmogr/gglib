//! What a run route refuses before anything is sent.

use gglib_core::domain::hub_chats::HubTurn;

use super::super::FarProxy;
use super::super::far_proxy_tests::{NOWHERE, credentials};
use crate::error::GuiError;

#[tokio::test]
async fn a_run_id_that_could_name_another_route_is_refused_before_sending() {
    let far = FarProxy::new(NOWHERE, &credentials()).unwrap();
    let turn = HubTurn {
        conversation_id: 1,
        content: "hi".to_owned(),
        images: Vec::new(),
        thinking: None,
        answer_saved: false,
        draw: false,
        image_model: None,
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
