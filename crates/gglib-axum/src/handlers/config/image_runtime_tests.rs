//! The install route's stream, with a stand-in install: the same SSE event
//! names and payloads llama.cpp's install sends, a failure as a `failed`
//! event in the install's words, and a second install refused while one
//! runs. A removal holds the same slot: refused while an install runs, and
//! refusing an install while it runs. No release is fetched.

use std::sync::atomic::AtomicBool;

use axum::response::IntoResponse;
use gglib_runtime::llama::InstallPhase;
use http_body_util::BodyExt;

use super::*;

/// The response body of `sse`, read to its end.
async fn body_of(sse: impl IntoResponse) -> String {
    let bytes = sse
        .into_response()
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

/// The `(event, data)` pairs of an SSE body.
fn frames(body: &str) -> Vec<(String, String)> {
    body.split("\n\n")
        .filter(|frame| !frame.trim().is_empty())
        .map(|frame| {
            let field = |name: &str| {
                frame
                    .lines()
                    .find_map(|line| line.strip_prefix(name))
                    .unwrap_or_default()
                    .to_owned()
            };
            (field("event: "), field("data: "))
        })
        .collect()
}

#[tokio::test]
async fn an_install_streams_llama_cpps_event_names_and_payloads() {
    static SLOT: AtomicBool = AtomicBool::new(false);
    let sse = install_stream(&SLOT, |tx| async move {
        let phase = InstallPhase::Download;
        let _ = tx.send(LlamaProgressEvent::PhaseStarted { phase }).await;
        let _ = tx
            .send(LlamaProgressEvent::Progress {
                downloaded: 5,
                total: 10,
                rate_bps: None,
                eta_seconds: None,
            })
            .await;
        let _ = tx.send(LlamaProgressEvent::PhaseCompleted { phase }).await;
        let version = "master-948-228c707".to_owned();
        let _ = tx.send(LlamaProgressEvent::Completed { version }).await;
        Ok(())
    });

    assert_eq!(
        frames(&body_of(sse).await),
        [
            (
                "phase_started",
                r#"{"type":"phase_started","phase":"download"}"#
            ),
            (
                "progress",
                r#"{"type":"progress","downloaded":5,"total":10}"#
            ),
            (
                "phase_completed",
                r#"{"type":"phase_completed","phase":"download"}"#
            ),
            (
                "completed",
                r#"{"type":"completed","version":"master-948-228c707"}"#
            ),
        ]
        .map(|(e, d)| (e.to_owned(), d.to_owned()))
    );
    assert!(
        !SLOT.load(Ordering::SeqCst),
        "the slot is free once it ends"
    );
}

#[tokio::test]
async fn a_failed_install_ends_in_a_failed_event_with_its_words() {
    static SLOT: AtomicBool = AtomicBool::new(false);
    let sse = install_stream(&SLOT, |_tx| async move {
        Err(GuiError::Internal(
            "Failed to install stable-diffusion.cpp: no asset".to_owned(),
        ))
    });

    assert_eq!(
        frames(&body_of(sse).await),
        [(
            "failed".to_owned(),
            r#"{"type":"failed","message":"internal error: Failed to install stable-diffusion.cpp: no asset"}"#
                .to_owned()
        )]
    );
}

#[tokio::test]
async fn a_second_install_is_refused_while_one_runs() {
    static SLOT: AtomicBool = AtomicBool::new(true);
    let sse = install_stream(&SLOT, |_tx| async move {
        panic!("a refused install never starts");
    });

    let frames = frames(&body_of(sse).await);

    assert_eq!(frames.len(), 1, "{frames:?}");
    assert_eq!(frames[0].0, "failed");
    assert!(frames[0].1.contains("already running"), "{frames:?}");
    assert!(
        SLOT.load(Ordering::SeqCst),
        "the running install keeps its slot"
    );
}

#[tokio::test]
async fn a_removal_is_refused_while_an_install_runs() {
    static SLOT: AtomicBool = AtomicBool::new(true);
    static RAN: AtomicBool = AtomicBool::new(false);

    let refused = holding_install_slot(&SLOT, async {
        RAN.store(true, Ordering::SeqCst);
        Ok::<(), GuiError>(())
    })
    .await;

    assert!(
        matches!(refused, Err(HttpError::Conflict(_))),
        "{refused:?}"
    );
    assert!(
        !RAN.load(Ordering::SeqCst),
        "a refused removal never starts"
    );
    assert!(
        SLOT.load(Ordering::SeqCst),
        "the running install keeps its slot"
    );
}

#[tokio::test]
async fn a_removal_holds_the_install_slot_until_it_ends() {
    static SLOT: AtomicBool = AtomicBool::new(false);

    let removed = holding_install_slot(&SLOT, async {
        let install = install_stream(&SLOT, |_tx| async move {
            panic!("no install starts during a removal");
        });
        let frames = frames(&body_of(install).await);
        assert_eq!(frames.len(), 1, "{frames:?}");
        assert_eq!(frames[0].0, "failed");
        Ok::<_, GuiError>("removed")
    })
    .await;

    assert_eq!(removed.ok(), Some("removed"));
    assert!(
        !SLOT.load(Ordering::SeqCst),
        "the slot is free once it ends"
    );
}
