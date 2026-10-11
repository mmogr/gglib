//! The daemon's drawing availability route hands its query to the rule.

use axum::Json;
use axum::extract::{Query, State};
use gglib_core::contracts::http::images::DrawingQuery;

use super::drawing;
use crate::handlers::agent::run_fixture::drawing_state;

/// `image_model` reaches the driver as the model chosen, and `far` still
/// refuses before the driver is asked.
#[tokio::test]
async fn the_route_asks_about_the_chosen_image_model() {
    let (_dir, state) = drawing_state().await;
    let ask = |query: DrawingQuery| drawing(State(state.clone()), Query(query));

    let Json(plain) = ask(DrawingQuery::default()).await;
    assert_eq!((plain.available, plain.model_id), (true, None));

    let Json(chosen) = ask(DrawingQuery {
        image_model: Some(7),
        ..DrawingQuery::default()
    })
    .await;
    assert_eq!((chosen.available, chosen.model_id), (true, Some(7)));

    let Json(far) = ask(DrawingQuery {
        far: true,
        image_model: Some(7),
        ..DrawingQuery::default()
    })
    .await;
    assert_eq!((far.available, far.model_id), (false, None));
}
