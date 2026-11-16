//! What the event bus hears of the endpoint (the proxy) as a model start
//! brings it up: `proxy_started` with its port when it goes from off to on,
//! whoever turned it on, and `proxy_stopped` when it goes from on to off;
//! once per change, and nothing for a start or a stop that changes nothing.
//!
//! A window showing the endpoint Off learns it is on from that event alone:
//! a model start answers with the model's port, not the endpoint's status.

use gglib_core::events::AppEvent;
use gglib_runtime::proxy::ProxyConfig;

use super::*;

/// `event` as `/api/events` sends it.
fn frame(event: &AppEvent) -> String {
    serde_json::to_string(event).expect("an event serializes")
}

/// Starting a model with the endpoint off turns it on, and says so once,
/// with the port it is on; a second start finds it on and says nothing.
#[tokio::test]
async fn starting_a_model_turns_the_endpoint_on_and_says_so_once() {
    let fx = Fixture::new(None, None).await;

    let first = fx
        .ops
        .start(fx.model_id, StartServerRequest::default())
        .await;
    let second = fx
        .ops
        .start(fx.model_id, StartServerRequest::default())
        .await;
    let frames = fx.frames();
    fx.proxy
        .stop()
        .await
        .expect("the proxy the start brought up");

    first.expect("the first start succeeds");
    second.expect("the second start succeeds");
    assert_eq!(
        frames,
        [frame(&AppEvent::proxy_started(fx.proxy_port))],
        "the endpoint came on once"
    );
}

/// Stopping the endpoint says so, and a model started after it turns it
/// on again and says that again: one event per change. A stop that finds
/// it already off says nothing.
#[tokio::test]
async fn each_change_of_the_endpoint_is_said_once() {
    let fx = Fixture::new(None, None).await;

    let stopped_while_off = fx.proxy.stop().await;
    fx.ops
        .start(fx.model_id, StartServerRequest::default())
        .await
        .expect("the first start succeeds");
    fx.proxy.stop().await.expect("the endpoint stops");
    fx.ops
        .start(fx.model_id, StartServerRequest::default())
        .await
        .expect("the start after the stop succeeds");
    let frames = fx.frames();
    fx.proxy
        .stop()
        .await
        .expect("the proxy the start brought up");

    assert!(stopped_while_off.is_err(), "{stopped_while_off:?}");
    assert_eq!(
        frames,
        [
            frame(&AppEvent::proxy_started(fx.proxy_port)),
            frame(&AppEvent::proxy_stopped()),
            frame(&AppEvent::proxy_started(fx.proxy_port)),
        ]
    );
}

/// The endpoint's own start says the same, once, whatever asked for it; a
/// second start, refused because it is already on, says nothing.
#[tokio::test]
async fn the_endpoints_own_start_says_it_is_on_once() {
    let fx = Fixture::new(None, None).await;
    let config = || ProxyConfig {
        host: "127.0.0.1".to_owned(),
        port: fx.proxy_port,
        ..ProxyConfig::default()
    };

    let address = fx
        .proxy
        .start(config(), None)
        .await
        .expect("the endpoint starts");
    let again = fx.proxy.start(config(), None).await;
    let frames = fx.frames();
    fx.proxy.stop().await.expect("the endpoint stops");

    assert!(matches!(again, Err(GuiError::Conflict(_))), "{again:?}");
    assert_eq!(frames, [frame(&AppEvent::proxy_started(address.port()))]);
}
