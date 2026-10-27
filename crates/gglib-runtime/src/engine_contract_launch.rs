//! Starting the pinned llama-server for the recorder, and asking it things.
//!
//! Every launch goes through gglib's own [`build_server_config`] and
//! [`build_command`], so the flags are the ones a user's launch gets. The
//! recorder adds only `--chat-template-file`, to try a template on a model
//! that does not carry it. Each process is killed when its [`Server`] drops,
//! a panic included.

use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

use gglib_core::cache_config::KvCacheType;
use serde_json::Value;

use crate::command::build_command;
use crate::server_config::{ServerConfigOptions, build_server_config};

/// The context every launch asks for, through gglib's own `-c`.
pub(super) const CONTEXT: u64 = 2048;

/// The ports the recorder may use, and no others.
const PORTS: std::ops::RangeInclusive<u16> = 19431..=19439;

/// `/props` `media_marker` is random per process unless this is set.
const MEDIA_MARKER: &str = "<__media__>";

/// The 260K-parameter story model the templates are tried on.
pub(super) const STORIES: &str = "stories260K-f32.gguf";
/// The 48 MB Gemma 3 that reads images, and its projector.
pub(super) const GEMMA: &str = "tinygemma3-Q8_0.gguf";
pub(super) const GEMMA_PROJECTOR: &str = "mmproj-tinygemma3.gguf";
/// The picture the image pair sends.
pub(super) const CAT: &str = "0_cat.png";

/// What the recorder is given: the installed engines, the tiny models with a
/// `templates/` folder beside them, and a scratch folder of its own.
pub(super) struct Inputs {
    pub(super) llama_server: PathBuf,
    pub(super) sd_server: PathBuf,
    pub(super) models: PathBuf,
    pub(super) scratch: PathBuf,
}

impl Inputs {
    /// `None` when `GGLIB_ENGINE_MODELS_DIR` is unset; the recorder then skips.
    pub(super) fn from_env(scratch: &Path) -> Option<Self> {
        Self::with_models("GGLIB_ENGINE_MODELS_DIR", scratch)
    }

    /// The installed engines, with the models from the folder `variable`
    /// names; `None` when it is unset.
    pub(super) fn with_models(variable: &str, scratch: &Path) -> Option<Self> {
        let models = PathBuf::from(std::env::var_os(variable)?);
        let engines = std::env::var_os("GGLIB_ENGINES_DIR")
            .map(PathBuf::from)
            .expect("GGLIB_ENGINES_DIR names the folder gglib installed the engines into");
        Some(Self {
            llama_server: engines.join(".llama/bin/llama-server"),
            sd_server: engines.join(".sd/bin/sd-server"),
            models,
            scratch: scratch.to_path_buf(),
        })
    }

    /// The scratch path `name`, which must stay inside the scratch folder.
    pub(super) fn scratch_path(&self, name: &str) -> PathBuf {
        let path = self.scratch.join(name);
        assert!(
            path.starts_with(&self.scratch) && !name.contains(".."),
            "{}",
            path.display()
        );
        path
    }

    /// Run an engine binary with `arg` and return its stdout and stderr,
    /// with the binary's path, which `--help` prints, cut to its name.
    pub(super) fn run(&self, binary: &Path, arg: &str) -> (String, String) {
        let out = self
            .clean(std::process::Command::new(binary))
            .arg(arg)
            .output()
            .unwrap_or_else(|e| panic!("{} {arg}: {e}", binary.display()));
        let path = binary.to_string_lossy();
        let name = binary.file_name().expect("a binary").to_string_lossy();
        let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).replace(&*path, &name);
        (text(&out.stdout), text(&out.stderr))
    }

    /// The environment every engine process gets: no `LLAMA_ARG_*` from the
    /// caller, an empty `XDG_CONFIG_HOME` so no user `config.ini` applies, and
    /// a fixed media marker.
    fn clean(&self, mut cmd: std::process::Command) -> std::process::Command {
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("LLAMA_ARG_") {
                cmd.env_remove(key);
            }
        }
        let config = self.scratch_path("xdg");
        std::fs::create_dir_all(&config).expect("make the empty config folder");
        cmd.env("XDG_CONFIG_HOME", config)
            .env("LLAMA_MEDIA_MARKER", MEDIA_MARKER)
            .env_remove("LLAMA_SERVER_SLOTS_DEBUG");
        cmd
    }
}

/// One launch: which model, with which catalog tags, template or projector
/// (a file beside the model), and whether `/slots` shows the last task's
/// grammar.
pub(super) struct Launch<'a> {
    pub(super) model: &'a str,
    pub(super) tags: &'a [&'a str],
    pub(super) template: Option<&'a str>,
    pub(super) projector: Option<&'a str>,
    pub(super) slots_debug: bool,
}

/// A running llama-server, killed on drop.
pub(super) struct Server {
    child: Child,
    base: String,
    client: reqwest::Client,
    log: PathBuf,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A port in [`PORTS`] nothing listens on. Probed with `SO_REUSEADDR`, as
/// llama-server binds, so a port the last launch left in `TIME_WAIT` counts.
fn free_port() -> u16 {
    let free = |port: u16| {
        let socket = tokio::net::TcpSocket::new_v4().ok()?;
        socket.set_reuseaddr(true).ok()?;
        socket.bind(([127, 0, 0, 1], port).into()).ok()?;
        socket.listen(1).ok()
    };
    PORTS
        .clone()
        .find(|&port| free(port).is_some())
        .expect("a free port in 19431-19439")
}

impl Server {
    /// Start llama-server as gglib would, and wait until `/health` is ok.
    pub(super) async fn start(inputs: &Inputs, launch: &Launch<'_>, n: usize) -> Self {
        // gglib's default q8_0 cache needs a head size divisible by 32;
        // stories260K's is 8, so it gets f16, through gglib's own option.
        let cache = (launch.model == STORIES).then_some(KvCacheType::F16);
        let options = ServerConfigOptions {
            context_size: Some(CONTEXT),
            cache_type_k: cache,
            cache_type_v: cache,
            ..ServerConfigOptions::default()
        };
        let model = inputs.models.join(launch.model);
        let projector = launch.projector.map(|file| inputs.models.join(file));
        let tags: Vec<String> = launch.tags.iter().map(|&t| t.to_owned()).collect();
        let config = build_server_config(1, "contract".to_owned(), model, 0, &tags, options)
            .with_mmproj(projector);
        let port = free_port();
        let mut cmd = inputs.clean(build_command(&inputs.llama_server, &config, port));
        if let Some(template) = launch.template {
            let path = inputs.models.join(format!("templates/{template}.jinja"));
            cmd.arg("--chat-template-file").arg(path);
        }
        if launch.slots_debug {
            cmd.env("LLAMA_SERVER_SLOTS_DEBUG", "1");
        }
        let log = inputs.scratch_path(&format!("llama-server-{n}.log"));
        let file = std::fs::File::create(&log).expect("create the server log");
        cmd.stdout(Stdio::null())
            .stderr(file)
            .current_dir(&inputs.scratch);
        let child = cmd.spawn().expect("start llama-server");
        let mut server = Self {
            child,
            base: format!("http://127.0.0.1:{port}"),
            client: gglib_proxy::loopback::client_builder()
                .timeout(Duration::from_mins(1))
                .build()
                .expect("an HTTP client"),
            log,
        };
        server.wait_healthy().await;
        server
    }

    async fn wait_healthy(&mut self) {
        let deadline = Instant::now() + Duration::from_mins(1);
        loop {
            if let Ok(Some(status)) = self.child.try_wait() {
                panic!("llama-server exited ({status}):\n{}", self.log_tail());
            }
            if let Ok((200, _)) = self.try_get("/health").await {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "llama-server never became healthy:\n{}",
                self.log_tail()
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    fn log_tail(&self) -> String {
        let log = std::fs::read_to_string(&self.log).unwrap_or_default();
        let lines: Vec<&str> = log.lines().collect();
        lines[lines.len().saturating_sub(30)..].join("\n")
    }

    async fn try_get(&self, path: &str) -> reqwest::Result<(u16, String)> {
        let response = self
            .client
            .get(format!("{}{path}", self.base))
            .send()
            .await?;
        let status = response.status().as_u16();
        Ok((status, response.text().await?))
    }

    /// `GET path`: its status and body.
    pub(super) async fn get(&self, path: &str) -> (u16, String) {
        self.try_get(path)
            .await
            .unwrap_or_else(|e| panic!("GET {path}: {e}"))
    }

    /// `GET path`, which must answer 200 with JSON.
    pub(super) async fn get_json(&self, path: &str) -> Value {
        let (status, body) = self.get(path).await;
        assert_eq!(status, 200, "GET {path}: {body}");
        serde_json::from_str(&body).unwrap_or_else(|e| panic!("GET {path}: {e}: {body}"))
    }

    /// `POST path` with a JSON body: its status and body.
    pub(super) async fn post(&self, path: &str, body: &Value) -> (u16, String) {
        self.try_post(path, body, Duration::from_mins(1))
            .await
            .unwrap_or_else(|e| panic!("POST {path}: {e}"))
    }

    /// `POST path` with a JSON body, waiting at most `timeout`.
    pub(super) async fn try_post(
        &self,
        path: &str,
        body: &Value,
        timeout: Duration,
    ) -> reqwest::Result<(u16, String)> {
        let response = self
            .client
            .post(format!("{}{path}", self.base))
            .json(body)
            .timeout(timeout)
            .send()
            .await?;
        let status = response.status().as_u16();
        Ok((status, response.text().await?))
    }

    /// `/slots` once no slot is processing. A slot answers before it is
    /// released, and only its release records the task `/slots` reports.
    pub(super) async fn idle_slots(&self) -> (Value, String) {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let (status, body) = self.get("/slots").await;
            assert_eq!(status, 200, "GET /slots: {body}");
            let slots: Value = serde_json::from_str(&body).expect("/slots is JSON");
            let busy = slots
                .as_array()
                .expect("/slots is an array")
                .iter()
                .any(|slot| slot["is_processing"] != Value::Bool(false));
            if !busy {
                return (slots, body);
            }
            assert!(Instant::now() < deadline, "a slot stayed busy: {body}");
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}
