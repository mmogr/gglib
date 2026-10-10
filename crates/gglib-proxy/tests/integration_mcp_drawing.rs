//! `builtin__generate_image` at the real proxy's `/mcp`: in the index only
//! with the `mcp_drawing` switch on and something to draw with, answered with
//! the image inline and one sentence, with progress for a caller that gave a
//! token, and refused when the switch is off whoever names it.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use gglib_core::ports::{
    GeneratedImage, ImageBatch, ImageError, ImageGenerationPort, ImageProgress, ImageRequest,
    ImageSize, ImageStage, InMemorySettings,
};
use gglib_core::{DevicePorts, ProxyAccessConfig, Settings};
use gglib_proxy::ServeConfig;
use reqwest::Client;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

mod fixtures;
use fixtures::spawn::{defaults, spawn};

const TOOL: &str = "builtin__generate_image";

/// An image driver that reports a short render and answers `result`,
/// counting every render it is asked for.
#[derive(Debug)]
struct Draws {
    /// What `drawing_model` answers: whether there is a model to draw with.
    can_draw: bool,
    result: Result<ImageBatch, ImageError>,
    renders: AtomicUsize,
    asked: Mutex<Vec<ImageRequest>>,
}

impl Draws {
    fn new(can_draw: bool, result: Result<ImageBatch, ImageError>) -> Arc<Self> {
        Arc::new(Self {
            can_draw,
            result,
            renders: AtomicUsize::new(0),
            asked: Mutex::new(Vec::new()),
        })
    }

    fn one_image() -> Arc<Self> {
        Self::new(
            true,
            Ok(ImageBatch {
                model: "sdxl".to_owned(),
                images: vec![GeneratedImage {
                    bytes: vec![1, 2, 3],
                    mime: "image/png",
                    width: 1024,
                    height: 1024,
                }],
                elapsed: Duration::from_secs(76),
            }),
        )
    }

    fn renders(&self) -> usize {
        self.renders.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl ImageGenerationPort for Draws {
    async fn generate(
        &self,
        request: ImageRequest,
        progress: mpsc::Sender<ImageProgress>,
    ) -> Result<ImageBatch, ImageError> {
        self.renders.fetch_add(1, Ordering::SeqCst);
        self.asked.lock().unwrap().push(request);
        let stages = [
            ImageStage::Queued {
                position: 1,
                behind: Some("an image render".to_owned()),
            },
            ImageStage::Loading,
            ImageStage::Sampling {
                pass: 1,
                step: 1,
                total: 2,
            },
            ImageStage::Sampling {
                pass: 1,
                step: 2,
                total: 2,
            },
            ImageStage::Decoding,
            ImageStage::Finishing,
        ];
        for stage in stages {
            let _ = progress.send(ImageProgress::stage(stage)).await;
        }
        self.result.clone()
    }

    async fn drawing_model(&self) -> Result<String, ImageError> {
        if self.can_draw {
            Ok("sdxl".to_owned())
        } else {
            Err(ImageError::Unavailable {
                reason: "no image model is installed; add one".to_owned(),
            })
        }
    }
}

/// A proxy whose settings hold `switch` and whose image driver is `images`,
/// with an `/mcp` session open on it.
struct Gateway {
    client: Client,
    base: String,
    session: String,
    cancel: CancellationToken,
}

impl Gateway {
    async fn open(switch: Option<bool>, images: Option<Arc<Draws>>) -> Self {
        let settings = Settings {
            mcp_drawing: switch,
            ..Settings::with_defaults()
        };
        let access = ProxyAccessConfig::default().with_devices(DevicePorts {
            images: images.map(|port| port as Arc<dyn ImageGenerationPort>),
            ..DevicePorts::default()
        });
        let proxy = spawn(ServeConfig {
            access,
            settings_repo: Arc::new(InMemorySettings::with(settings)),
            ..defaults().await
        })
        .await;
        let client = Client::new();
        let opened = client
            .post(format!("{}/mcp", proxy.base))
            .json(&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}))
            .send()
            .await
            .unwrap();
        let session = opened.headers()["mcp-session-id"]
            .to_str()
            .unwrap()
            .to_owned();
        Self {
            client,
            base: proxy.base,
            session,
            cancel: proxy.cancel,
        }
    }

    /// Call the meta-tool `name`, and return every JSON-RPC message the
    /// response carried, in order: an SSE response's frames, or the one JSON
    /// body.
    async fn call(&self, name: &str, arguments: Value, meta: Option<Value>) -> Vec<Value> {
        let mut params = json!({"name": name, "arguments": arguments});
        if let Some(meta) = meta {
            params["_meta"] = meta;
        }
        let response = self
            .client
            .post(format!("{}/mcp", self.base))
            .header("mcp-session-id", &self.session)
            .json(&json!({"jsonrpc": "2.0", "id": 9, "method": "tools/call", "params": params}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let streamed = response.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/event-stream");
        let body = response.text().await.unwrap();
        if !streamed {
            return vec![serde_json::from_str(&body).unwrap()];
        }
        body.split("\n\n")
            .filter_map(|frame| frame.lines().find_map(|line| line.strip_prefix("data: ")))
            .map(|data| serde_json::from_str(data).unwrap())
            .collect()
    }

    /// The tool ids `search_tools` lists for an empty query.
    async fn listed(&self) -> Vec<String> {
        let frames = self.call("search_tools", json!({"query": ""}), None).await;
        let text = frames[0]["result"]["content"][0]["text"].as_str().unwrap();
        let summaries: Vec<Value> = serde_json::from_str(text).unwrap();
        summaries
            .iter()
            .map(|s| s["tool_id"].as_str().unwrap().to_owned())
            .collect()
    }

    async fn invoke(&self, arguments: Value, meta: Option<Value>) -> Vec<Value> {
        self.call(
            "invoke_tool",
            json!({"tool_id": TOOL, "arguments": arguments}),
            meta,
        )
        .await
    }
}

impl Drop for Gateway {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

fn progress_frames(frames: &[Value]) -> Vec<&Value> {
    frames
        .iter()
        .filter(|f| f["method"] == "notifications/progress")
        .collect()
}

/// Off unless set: a proxy that could draw lists nothing while the setting
/// is absent or false, and lists the tool, with its schema, once it is on.
#[tokio::test]
async fn the_tool_is_listed_only_with_the_switch_on() {
    for off in [None, Some(false)] {
        let gateway = Gateway::open(off, Some(Draws::one_image())).await;
        assert_eq!(gateway.listed().await, Vec::<String>::new(), "{off:?}");
        let schema = gateway
            .call("get_tool_schema", json!({"tool_id": TOOL}), None)
            .await;
        assert_eq!(schema[0]["error"]["code"], -32602, "{off:?}");
    }

    let gateway = Gateway::open(Some(true), Some(Draws::one_image())).await;
    assert_eq!(gateway.listed().await, [TOOL]);
    let schema = gateway
        .call("get_tool_schema", json!({"tool_id": TOOL}), None)
        .await;
    let schema: Value =
        serde_json::from_str(schema[0]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(schema["required"], json!(["prompt"]));
    assert!(
        schema["properties"].get("model").is_none(),
        "the driver chooses the model"
    );
}

/// The switch alone is not enough: with no image driver (a proxy outside the
/// daemon), or a driver with nothing to draw with, the tool is not listed.
#[tokio::test]
async fn the_tool_is_listed_only_when_drawing_is_available() {
    let no_driver = Gateway::open(Some(true), None).await;
    assert_eq!(no_driver.listed().await, Vec::<String>::new());

    let no_model = Gateway::open(
        Some(true),
        Some(Draws::new(false, Err(ImageError::DeadlineExceeded))),
    )
    .await;
    assert_eq!(no_model.listed().await, Vec::<String>::new());
}

/// The answer is MCP's: each image as an `image` item with its bytes, then
/// one text item. The request reaches the driver as asked, with no model.
#[tokio::test]
async fn an_invoke_answers_the_image_inline_and_one_sentence() {
    let port = Draws::one_image();
    let gateway = Gateway::open(Some(true), Some(Arc::clone(&port))).await;

    let frames = gateway
        .invoke(
            json!({"prompt": " a red fox ", "size": "1024x768", "n": 2, "seed": 9}),
            None,
        )
        .await;

    let answer = frames.last().unwrap();
    assert_eq!(answer["id"], 9);
    assert_eq!(
        answer["result"],
        json!({"content": [
            {"type": "image", "data": "AQID", "mimeType": "image/png"},
            {"type": "text", "text": "Drew 1 image, 1024x1024 PNG, with sdxl in 76 s; it is \
                                      attached to this result, and gglib kept no copy."},
        ]})
    );
    assert_eq!(
        *port.asked.lock().unwrap(),
        [ImageRequest {
            model: None,
            prompt: "a red fox".to_owned(),
            size: Some(ImageSize {
                width: 1024,
                height: 768
            }),
            n: 2,
            seed: Some(9),
        }]
    );
}

/// A caller that sent `_meta.progressToken` hears the render's progress under
/// that token, before the answer, with a count that only rises.
#[tokio::test]
async fn progress_frames_carry_the_callers_token() {
    for token in [json!("tok-7"), json!(41)] {
        let gateway = Gateway::open(Some(true), Some(Draws::one_image())).await;

        let frames = gateway
            .invoke(
                json!({"prompt": "a red fox"}),
                Some(json!({"progressToken": token})),
            )
            .await;

        let (answer, before) = frames.split_last().unwrap();
        assert!(answer["result"]["content"].is_array(), "{answer}");
        let progress = progress_frames(before);
        assert_eq!(
            progress.len(),
            before.len(),
            "only progress precedes the answer"
        );
        assert_eq!(
            progress.len(),
            6,
            "queued, loading, two steps, decoding, finishing"
        );
        for frame in &progress {
            assert_eq!(frame["jsonrpc"], "2.0");
            assert!(frame.get("id").is_none(), "a notification has no id");
            assert_eq!(frame["params"]["progressToken"], token);
        }
        let counts: Vec<u64> = progress
            .iter()
            .map(|f| f["params"]["progress"].as_u64().unwrap())
            .collect();
        assert_eq!(counts, [1, 2, 3, 4, 5, 6]);
        assert_eq!(progress[0]["params"].get("total"), None);
        assert_eq!(progress[5]["params"]["total"], 6);
        assert_eq!(
            progress[0]["params"]["message"],
            "queued, place 1 in line behind an image render"
        );
        assert_eq!(
            progress[3]["params"]["message"],
            "sampling step 2 of 2, image 1 of 1"
        );
    }
}

/// A caller that asked for no progress gets none: the answer is the only
/// message on the response.
#[tokio::test]
async fn no_progress_frame_is_sent_without_a_token() {
    let port = Draws::one_image();
    let gateway = Gateway::open(Some(true), Some(Arc::clone(&port))).await;

    let frames = gateway.invoke(json!({"prompt": "a red fox"}), None).await;

    assert_eq!(port.renders(), 1);
    assert_eq!(frames.len(), 1, "{frames:?}");
    assert!(frames[0]["result"]["content"].is_array());

    // A token that is neither a string nor a number is no token.
    let frames = gateway
        .invoke(
            json!({"prompt": "a red fox"}),
            Some(json!({"progressToken": {"a": 1}})),
        )
        .await;
    assert_eq!(frames.len(), 1, "{frames:?}");
}

/// The switch gates the call, not only the list: with it off, a caller that
/// names the tool anyway is refused, told why, and nothing is drawn.
#[tokio::test]
async fn an_invoke_with_the_switch_off_is_refused_and_draws_nothing() {
    for off in [None, Some(false)] {
        let port = Draws::one_image();
        let gateway = Gateway::open(off, Some(Arc::clone(&port))).await;

        let frames = gateway
            .invoke(
                json!({"prompt": "a red fox"}),
                Some(json!({"progressToken": "t"})),
            )
            .await;

        assert_eq!(frames.len(), 1, "{frames:?}");
        assert_eq!(frames[0]["error"]["code"], -32602);
        assert_eq!(
            frames[0]["error"]["message"],
            "Unknown tool: 'builtin__generate_image'. drawing through /mcp is switched off; \
             turn it on in Settings, or run `gglib config settings set --mcp-drawing true`"
        );
        assert!(frames[0].get("result").is_none());
        assert_eq!(port.renders(), 0, "{off:?}");
    }
}

/// With the switch on and nothing to draw with, the refusal carries the
/// driver's reason, and still nothing is asked of it.
#[tokio::test]
async fn an_invoke_with_nothing_to_draw_with_is_refused_with_the_reason() {
    let port = Draws::new(false, Err(ImageError::DeadlineExceeded));
    let gateway = Gateway::open(Some(true), Some(Arc::clone(&port))).await;

    let frames = gateway.invoke(json!({"prompt": "a red fox"}), None).await;

    assert_eq!(frames[0]["error"]["code"], -32602);
    assert_eq!(
        frames[0]["error"]["message"],
        "Unknown tool: 'builtin__generate_image'. no image model is installed; add one"
    );
    assert_eq!(port.renders(), 0);
}

/// What the caller's model can put right comes back as a result it reads,
/// `isError` set: a call with no prompt (nothing drawn), and a failed render.
#[tokio::test]
async fn a_bad_call_and_a_failed_render_are_error_results() {
    let port = Draws::new(
        true,
        Err(ImageError::Failed {
            message: "out of memory".to_owned(),
        }),
    );
    let gateway = Gateway::open(Some(true), Some(Arc::clone(&port))).await;

    let no_prompt = gateway.invoke(json!({"size": "1024x1024"}), None).await;
    assert_eq!(
        no_prompt[0]["result"],
        json!({"content": [{"type": "text", "text":
            "generate_image needs a prompt: describe the picture in detail"}], "isError": true})
    );
    assert_eq!(port.renders(), 0);

    let too_many = gateway
        .invoke(json!({"prompt": "a fox", "n": 5}), None)
        .await;
    assert_eq!(too_many[0]["result"]["isError"], true);
    assert_eq!(port.renders(), 0);

    let failed = gateway.invoke(json!({"prompt": "a fox"}), None).await;
    assert_eq!(failed[0]["result"]["isError"], true);
    assert_eq!(
        failed[0]["result"]["content"][0]["text"],
        "the image runtime could not draw this: out of memory; try another prompt or size, or retry"
    );
    assert_eq!(port.renders(), 1);
}
