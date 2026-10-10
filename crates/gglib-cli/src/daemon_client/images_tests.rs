//! Drawing through the daemon: the body sent, each event read back as a
//! report or an image, and each refusal as the daemon's code and words.

use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use futures_util::stream;
use gglib_core::contracts::http::images::ImageStreamEvent;
use gglib_core::ports::{
    ImageError, ImageGenerationPort, ImageProgress, ImageRequest, ImageSize, ImageStage,
};
use tokio::sync::mpsc;

use super::{DaemonImageGenerator, body_of, progress_of, read_render, refusal};
use crate::daemon_client::{DaemonHandle, STAND_IN_PORT};

/// A PNG's signature and `IHDR` at `width` by `height`.
fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
    bytes.extend(b"IHDR");
    bytes.extend(width.to_be_bytes());
    bytes.extend(height.to_be_bytes());
    bytes.extend([8, 6, 0, 0, 0]);
    bytes
}

fn progress(stage: &str) -> ImageStreamEvent {
    ImageStreamEvent::Progress {
        stage: stage.to_owned(),
        pass: None,
        step: None,
        total: None,
        position: None,
        behind: None,
        frame_b64: None,
    }
}

fn sampling(pass: u32, step: u32, total: u32, frame: Option<&str>) -> ImageStreamEvent {
    ImageStreamEvent::Progress {
        stage: "sampling".to_owned(),
        pass: Some(pass),
        step: Some(step),
        total: Some(total),
        position: None,
        behind: None,
        frame_b64: frame.map(str::to_owned),
    }
}

fn completed(width: u32, height: u32) -> ImageStreamEvent {
    ImageStreamEvent::Completed {
        b64_json: BASE64.encode(png(width, height)),
        size: format!("{width}x{height}"),
        output_format: "png".to_owned(),
        created_at: 1,
        model: Some("flux1-schnell".to_owned()),
    }
}

/// `events` as the daemon streams them.
fn sse(events: &[ImageStreamEvent]) -> String {
    let mut wire = String::new();
    for event in events {
        wire.push_str("event: x\ndata: ");
        wire.push_str(&serde_json::to_string(event).unwrap());
        wire.push_str("\n\n");
    }
    wire
}

fn drain(rx: &mut mpsc::Receiver<ImageProgress>) -> Vec<ImageStage> {
    let mut stages = Vec::new();
    while let Ok(report) = rx.try_recv() {
        stages.push(report.stage);
    }
    stages
}

#[test]
fn the_body_asks_for_a_stream_of_what_was_asked() {
    let request = ImageRequest {
        model: Some("flux".into()),
        size: Some(ImageSize {
            width: 512,
            height: 768,
        }),
        n: 2,
        seed: Some(3),
        ..ImageRequest::new("a cat")
    };
    assert_eq!(
        serde_json::to_value(body_of(&request)).unwrap(),
        serde_json::json!({"model": "flux", "prompt": "a cat", "n": 2,
                           "size": "512x768", "seed": 3, "stream": true})
    );
}

/// Every stage reads back as its report, a step with its frame; any other
/// event, or a stage it does not know, is no report.
#[test]
fn each_progress_event_is_its_report() {
    let queued = ImageStreamEvent::Progress {
        stage: "queued".into(),
        pass: None,
        step: None,
        total: None,
        position: Some(2),
        behind: Some("an image render".into()),
        frame_b64: None,
    };
    assert_eq!(
        progress_of(&queued).unwrap().stage,
        ImageStage::Queued {
            position: 2,
            behind: Some("an image render".into())
        }
    );
    let step = sampling(2, 3, 4, Some("AAA"));
    let report = progress_of(&step).unwrap();
    assert_eq!(
        report.stage,
        ImageStage::Sampling {
            pass: 2,
            step: 3,
            total: 4
        }
    );
    assert_eq!(&*report.preview.unwrap().b64, "AAA");
    for (name, stage) in [
        ("loading", ImageStage::Loading),
        ("decoding", ImageStage::Decoding),
        ("finishing", ImageStage::Finishing),
    ] {
        assert_eq!(progress_of(&progress(name)).unwrap().stage, stage);
    }
    assert!(progress_of(&progress("dreaming")).is_none());
    assert!(progress_of(&completed(8, 8)).is_none());
}

/// A stream in pieces: the reports go on, the images come back with their
/// sizes read from them.
#[tokio::test]
async fn a_render_stream_reads_as_reports_and_images() {
    let wire = sse(&[
        progress("loading"),
        sampling(1, 1, 4, None),
        ImageStreamEvent::PartialImage {
            b64_json: "AA".into(),
            partial_image_index: 0,
            size: "auto".into(),
            output_format: "png".into(),
            created_at: 1,
        },
        progress("decoding"),
        completed(1024, 768),
        completed(1024, 768),
    ]);
    let chunks: Vec<Result<Vec<u8>, std::io::Error>> =
        wire.as_bytes().chunks(7).map(|c| Ok(c.to_vec())).collect();
    let (tx, mut rx) = mpsc::channel(16);
    let rendered = read_render(stream::iter(chunks), &tx)
        .await
        .unwrap()
        .expect("drawn");
    assert_eq!(rendered.model.as_deref(), Some("flux1-schnell"));
    let images = rendered.images;
    assert_eq!(images.len(), 2);
    assert_eq!((images[0].width, images[0].height), (1024, 768));
    assert_eq!(images[0].bytes, png(1024, 768));
    assert_eq!(
        drain(&mut rx),
        [
            ImageStage::Loading,
            ImageStage::Sampling {
                pass: 1,
                step: 1,
                total: 4
            },
            ImageStage::Decoding
        ]
    );
}

/// An error event ends the render with the daemon's code and words; a
/// stream that ends with no image is a failure.
#[tokio::test]
async fn an_error_event_is_the_daemons_refusal() {
    let wire = format!(
        "{}event: error\ndata: {}\n\n",
        sse(&[progress("loading")]),
        r#"{"error":{"message":"the render made no progress","type":"server_error","code":"image_render_stalled"}}"#
    );
    let (tx, _rx) = mpsc::channel(16);
    let chunks: Vec<Result<Vec<u8>, std::io::Error>> = vec![Ok(wire.into_bytes())];
    let refused = read_render(stream::iter(chunks), &tx)
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(refused.code(), Some("image_render_stalled"));
    assert_eq!(refused.to_string(), "the render made no progress");

    let empty: Vec<Result<Vec<u8>, std::io::Error>> =
        vec![Ok(sse(&[progress("loading")]).into_bytes())];
    let nothing = read_render(stream::iter(empty), &tx)
        .await
        .unwrap()
        .unwrap_err();
    assert!(matches!(nothing, ImageError::Failed { .. }), "{nothing:?}");
}

#[test]
fn a_refused_request_carries_the_daemons_status_code_and_words() {
    let refused = refusal(
        400,
        r#"{"error":{"message":"7x7 is not a size this model draws","type":"invalid_request_error","code":"invalid_image_size"}}"#,
    );
    assert_eq!(refused.code(), Some("invalid_image_size"));
    assert_eq!(refused.http_status(), 400);
    assert_eq!(refused.to_string(), "7x7 is not a size this model draws");
    assert_eq!(refusal(502, "").to_string(), "the daemon answered 502");
}

/// A stand-in daemon answering one request with `status`, `content_type`
/// and `body`, keeping the request line and body it was sent.
fn stand_in(status: u16, content_type: &str, body: String) -> (u16, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let kept = Arc::clone(&seen);
    let content_type = content_type.to_owned();
    std::thread::spawn(move || {
        let Ok((mut socket, _)) = listener.accept() else {
            return;
        };
        let mut head = Vec::new();
        let mut byte = [0_u8; 1];
        while !head.ends_with(b"\r\n\r\n") && socket.read(&mut byte).is_ok_and(|n| n == 1) {
            head.push(byte[0]);
        }
        let head = String::from_utf8_lossy(&head).into_owned();
        let length: usize = head
            .lines()
            .filter_map(|l| l.split_once(':'))
            .find(|(n, _)| n.eq_ignore_ascii_case("content-length"))
            .and_then(|(_, v)| v.trim().parse().ok())
            .unwrap_or(0);
        let mut sent = vec![0_u8; length];
        let _ = socket.read_exact(&mut sent);
        let mut log = kept.lock().unwrap();
        log.push(head.lines().next().unwrap_or_default().to_owned());
        log.push(String::from_utf8_lossy(&sent).into_owned());
        drop(log);
        let _ = write!(
            socket,
            "HTTP/1.1 {status} X\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\n\
             connection: close\r\n\r\n{body}",
            body.len()
        );
    });
    (port, seen)
}

fn generator() -> DaemonImageGenerator {
    DaemonImageGenerator::new(DaemonHandle {
        client: reqwest::Client::new(),
        api_key: None,
    })
}

/// The whole call: posted to the daemon's drawing route with `stream`, read
/// back as reports and images.
#[tokio::test]
async fn a_render_is_posted_to_the_daemon_and_read_back() {
    let (port, seen) = stand_in(
        200,
        "text/event-stream",
        sse(&[progress("loading"), completed(512, 512)]),
    );
    let (tx, mut rx) = mpsc::channel(16);
    let batch = STAND_IN_PORT
        .scope(port, generator().generate(ImageRequest::new("a fox"), tx))
        .await
        .expect("drawn");
    assert_eq!(batch.images.len(), 1);
    assert_eq!(batch.images[0].width, 512);
    assert_eq!(batch.model, "flux1-schnell", "the model the daemon named");
    assert_eq!(drain(&mut rx), [ImageStage::Loading]);
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen[0], "POST /api/images/generations HTTP/1.1");
    let sent: serde_json::Value = serde_json::from_str(&seen[1]).unwrap();
    assert_eq!(sent["stream"], true);
}

/// A refused call is the daemon's refusal, with its status.
#[tokio::test]
async fn a_refused_render_is_the_daemons_refusal() {
    let (port, _) = stand_in(
        400,
        "application/json",
        r#"{"error":{"message":"no image model","type":"invalid_request_error","code":"drawing_unavailable"}}"#.to_owned(),
    );
    let (tx, _rx) = mpsc::channel(16);
    let refused = STAND_IN_PORT
        .scope(port, generator().generate(ImageRequest::new("a fox"), tx))
        .await
        .unwrap_err();
    assert_eq!(
        (refused.code(), refused.http_status()),
        (Some("drawing_unavailable"), 400)
    );
}

/// Whether the daemon can draw is the daemon's answer: its model, its
/// reason, and "cannot" from a daemon too old to have the route.
#[tokio::test]
async fn the_drawing_model_is_the_daemons_answer() {
    let ask = |status, body: &str| {
        let (port, seen) = stand_in(status, "application/json", body.to_owned());
        async move {
            let answer = STAND_IN_PORT.scope(port, generator().drawing_model()).await;
            let line = seen.lock().unwrap()[0].clone();
            assert_eq!(line, "GET /api/images/drawing HTTP/1.1");
            answer
        }
    };
    let can = ask(200, r#"{"available":true,"model":"sdxl"}"#).await;
    assert_eq!(can.unwrap(), "sdxl");

    let cannot = r#"{"available":false,"code":"drawing_unavailable","reason":"no image model"}"#;
    let cannot = ask(200, cannot).await.unwrap_err();
    assert_eq!(cannot.code(), Some("drawing_unavailable"));
    assert_eq!(cannot.to_string(), "no image model");

    let old = ask(404, "{}").await.unwrap_err();
    assert!(old.to_string().contains("answered 404"), "{old}");
}
