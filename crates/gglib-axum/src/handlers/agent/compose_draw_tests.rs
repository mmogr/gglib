//! Draw is the only thing that offers the image tool: no tool filter, `null`
//! included, reaches it without `draw`; with `draw` a filter gains exactly
//! the qualified name; and a request that cannot draw is refused before a
//! slot is taken or a row written.

use std::collections::HashSet;

use serde_json::{Value, json};

use super::{DRAW_TOOL, refuse_unavailable_drawing, with_drawing};
use crate::error::HttpError;
use crate::handlers::agent::run::create_run;
use crate::handlers::agent::run_fixture::{conversation, drawing_state, saved, state};
use crate::handlers::agent::turn_fixture::{TOOL, model, page, sent};

/// The names of the tools a request to the model offers.
fn offered(body: &Value) -> Vec<String> {
    let mut names: Vec<String> = body["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|tool| tool["function"]["name"].as_str().unwrap().to_owned())
        .collect();
    names.sort();
    names
}

fn set(names: &[&str]) -> HashSet<String> {
    names.iter().map(|n| (*n).to_owned()).collect()
}

/// Without `draw` the image tool is in no list: not with every tool asked
/// for (`tool_filter: null`), not when the filter names it outright.
#[tokio::test]
async fn without_draw_no_tool_filter_reaches_the_image_tool() {
    let (_dir, state) = drawing_state().await;
    let id = model(&state, |_| {}).await;

    let every = sent(
        &state,
        id,
        page(&state, json!({ "tool_filter": null })).await,
    )
    .await;
    let names = offered(&every);
    assert!(
        names.contains(&TOOL.to_owned()),
        "every other tool: {names:?}"
    );
    assert!(
        !names.iter().any(|n| n.contains("generate_image")),
        "{names:?}"
    );

    let said_false = json!({ "tool_filter": null, "draw": false });
    let names = offered(&sent(&state, id, page(&state, said_false).await).await);
    assert!(
        !names.iter().any(|n| n.contains("generate_image")),
        "{names:?}"
    );

    let summoning = json!({ "tool_filter": [DRAW_TOOL, "generate_image", TOOL] });
    let names = offered(&sent(&state, id, page(&state, summoning).await).await);
    assert_eq!(names, [TOOL], "a filter cannot summon the image tool");
}

/// With `draw` the model is offered the image tool: beside every tool, and
/// beside exactly the tools a filter names.
#[tokio::test]
async fn draw_offers_the_image_tool_beside_what_the_filter_names() {
    let (_dir, state) = drawing_state().await;
    let id = model(&state, |_| {}).await;

    let every = json!({ "tool_filter": null, "draw": true });
    let names = offered(&sent(&state, id, page(&state, every).await).await);
    assert!(names.contains(&DRAW_TOOL.to_owned()), "{names:?}");
    assert!(names.contains(&TOOL.to_owned()), "{names:?}");

    let listed = json!({ "tool_filter": [TOOL], "draw": true });
    let names = offered(&sent(&state, id, page(&state, listed).await).await);
    assert_eq!(names, [DRAW_TOOL, TOOL]);

    let none = json!({ "tool_filter": [], "draw": true });
    let names = offered(&sent(&state, id, page(&state, none).await).await);
    assert_eq!(names, [DRAW_TOOL]);
}

/// A filter gains exactly the qualified name, never the bare one, which
/// would also match an MCP server's tool called `generate_image`.
#[test]
fn draw_adds_exactly_the_qualified_name_to_a_filter() {
    assert_eq!(DRAW_TOOL, "builtin:generate_image");
    assert_eq!(
        with_drawing(Some(set(&["3:search"])), true),
        Some(set(&["3:search", "builtin:generate_image"]))
    );
    assert_eq!(with_drawing(Some(set(&[])), true), Some(set(&[DRAW_TOOL])));
    assert_eq!(
        with_drawing(Some(set(&["3:search"])), false),
        Some(set(&["3:search"]))
    );
    assert_eq!(
        with_drawing(None, true),
        None,
        "every tool stays every tool"
    );
    assert_eq!(with_drawing(None, false), None);
}

fn refusal(result: Result<(), HttpError>) -> (u16, &'static str, String) {
    match result {
        Err(HttpError::Coded {
            status,
            code,
            message,
        }) => (status.as_u16(), code, message),
        other => panic!("not a coded refusal: {other:?}"),
    }
}

/// A message sent with Draw pressed is refused when its model is on another
/// machine, even where this machine can draw, and when this machine cannot;
/// one sent without is never asked.
#[tokio::test]
async fn draw_is_refused_where_it_cannot_draw() {
    let (_dir, can) = drawing_state().await;
    let here = page(&can, json!({ "draw": true })).await;
    assert!(refuse_unavailable_drawing(&can, &here).await.is_ok());

    let machine = json!({ "kind": "paired", "fingerprint": "0a1b2c3d4e5f" });
    let far = json!({ "draw": true, "far": { "machine": machine, "id": 7 } });
    let far = page(&can, far).await;
    let (status, code, why) = refusal(refuse_unavailable_drawing(&can, &far).await);
    assert_eq!((status, code), (400, "drawing_unavailable"));
    assert!(why.contains("another machine"), "{why}");

    let (_dir, cannot) = state().await;
    let asked = page(&cannot, json!({ "draw": true })).await;
    let (status, code, why) = refusal(refuse_unavailable_drawing(&cannot, &asked).await);
    assert_eq!((status, code), (400, "drawing_unavailable"));
    assert!(why.contains("not installed"), "{why}");
    let plain = page(&cannot, json!({})).await;
    assert!(refuse_unavailable_drawing(&cannot, &plain).await.is_ok());
}

/// The run's door refuses it before a slot is taken, a run made or a row
/// written.
#[tokio::test]
async fn a_run_that_cannot_draw_writes_nothing_and_takes_no_slot() {
    let (_dir, state) = state().await;
    let chat = conversation(&state).await;
    let body = json!({
        "port": 0,
        "messages": [{ "role": "user", "content": "draw a fox" }],
        "conversation_id": chat,
        "draw": true,
    });

    let refused = create_run(&state, "r1", body).await;

    match refused {
        Err(HttpError::Coded { status, code, .. }) => {
            assert_eq!((status.as_u16(), code), (400, "drawing_unavailable"));
        }
        other => panic!("not a coded refusal: {other:?}"),
    }
    assert!(saved(&state, chat).await.is_empty(), "no row was written");
    assert_eq!(state.agent_semaphore.available_permits(), 1);
    let listed =
        gglib_core::ports::RunsPort::list(state.runs.as_ref(), &gglib_core::ports::RunScope::Local);
    assert!(listed.runs.is_empty(), "no run was made");
}

/// The chat route refuses it too, before its slot is taken.
#[tokio::test]
async fn the_chat_route_refuses_a_draw_it_cannot_draw_before_its_slot() {
    let (_dir, state) = state().await;
    let request = page(&state, json!({ "draw": true })).await;

    let answered =
        crate::handlers::agent::chat(axum::extract::State(state.clone()), axum::Json(request))
            .await;

    match answered {
        Err(HttpError::Coded { status, code, .. }) => {
            assert_eq!((status.as_u16(), code), (400, "drawing_unavailable"));
        }
        Err(other) => panic!("not a coded refusal: {other:?}"),
        Ok(_) => panic!("the chat route drew"),
    }
    assert_eq!(state.agent_semaphore.available_permits(), 1);
}
