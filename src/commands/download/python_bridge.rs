use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use indicatif::{HumanBytes, ProgressBar, ProgressDrawTarget, ProgressState, ProgressStyle};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio::signal;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::utils::paths::get_data_root;

use super::file_ops::ProgressCallback;

const FAST_CANCELLED_MSG: &str = "fast download cancelled by user";

const PY_HELPER_SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/scripts/hf_xet_downloader.py"
));
const ENV_MARKER_NAME: &str = ".gglib-hf-xet.json";
const PY_REQUIREMENTS: &[&str] = &["huggingface_hub>=1.1.5", "hf_xet>=0.6.0"];
#[cfg(target_os = "windows")]
const PYTHON_CANDIDATES: &[&str] = &["python"];
#[cfg(not(target_os = "windows"))]
const PYTHON_CANDIDATES: &[&str] = &["python3", "python"];

/// Request payload for running the fast downloader.
pub struct FastDownloadRequest<'a> {
    pub repo_id: &'a str,
    pub revision: &'a str,
    pub repo_type: &'a str,
    pub destination: &'a Path,
    pub files: &'a [String],
    pub token: Option<&'a str>,
    pub force: bool,
    pub progress: Option<&'a ProgressCallback>,
}

pub(crate) async fn ensure_fast_helper_ready() -> Result<()> {
    PythonHelper::prepare().await.map(|_| ())
}

/// Try to download files using the embedded Python helper.
pub async fn run_fast_download(request: &FastDownloadRequest<'_>) -> Result<()> {
    if request.files.is_empty() {
        return Ok(());
    }

    let helper = PythonHelper::prepare().await.with_context(
        || "Fast download helper is unavailable. Run `make setup` to install Python dependencies.",
    )?;

    helper.run(request).await.map_err(|err| {
        if err.to_string().contains(FAST_CANCELLED_MSG) {
            err
        } else {
            anyhow!("Fast downloader failed: {err}. Run `make setup` to reinstall dependencies.")
        }
    })
}

struct PythonHelper {
    env_dir: PathBuf,
    script_path: PathBuf,
}

struct CliProgressPrinter {
    inner: ProgressRender,
}

enum ProgressRender {
    Fancy(FancyProgress),
    Plain(PlainProgress),
}

fn finish_progress(printer: &mut Option<CliProgressPrinter>) {
    if let Some(p) = printer.as_mut() {
        p.finish();
    }
}

impl CliProgressPrinter {
    fn new() -> Self {
        if io::stdout().is_terminal() {
            Self {
                inner: ProgressRender::Fancy(FancyProgress::new()),
            }
        } else {
            Self {
                inner: ProgressRender::Plain(PlainProgress::new()),
            }
        }
    }

    fn update(&mut self, label: Option<&str>, downloaded: u64, total: u64) {
        match &mut self.inner {
            ProgressRender::Fancy(inner) => inner.update(label, downloaded, total),
            ProgressRender::Plain(inner) => inner.update(label, downloaded, total),
        }
    }

    fn finish(&mut self) {
        match &mut self.inner {
            ProgressRender::Fancy(inner) => inner.finish(),
            ProgressRender::Plain(inner) => inner.finish(),
        }
    }
}

struct FancyProgress {
    bar: ProgressBar,
    saw_length: bool,
    last_label: Option<String>,
}

impl FancyProgress {
    fn new() -> Self {
        let bar = ProgressBar::with_draw_target(None, ProgressDrawTarget::stdout());
        bar.set_style(Self::spinner_style());
        bar.set_message("Preparing fast download".to_string());
        bar.enable_steady_tick(Duration::from_millis(120));
        Self {
            bar,
            saw_length: false,
            last_label: None,
        }
    }

    fn update(&mut self, label: Option<&str>, downloaded: u64, total: u64) {
        let label_text = label.filter(|s| !s.is_empty()).unwrap_or("fast download");
        if total == 0 {
            self.bar
                .set_message(format!("{} (preparing...)", Self::format_label(label_text)));
            self.last_label = None;
            self.bar.tick();
            return;
        }

        if self.last_label.as_deref() != Some(label_text) {
            self.bar.set_message(Self::format_label(label_text));
            self.last_label = Some(label_text.to_string());
        }

        if !self.saw_length {
            self.bar.set_style(Self::bar_style());
            self.bar.set_length(total);
            self.saw_length = true;
        } else if let Some(current) = self.bar.length() {
            if current != total {
                self.bar.set_length(total);
            }
        } else {
            self.bar.set_length(total);
        }

        self.bar.set_position(downloaded.min(total));
    }

    fn finish(&mut self) {
        self.bar.finish_and_clear();
    }

    fn spinner_style() -> ProgressStyle {
        ProgressStyle::with_template("⚡ {msg} {spinner}").unwrap()
    }

    fn bar_style() -> ProgressStyle {
        ProgressStyle::with_template(
            "⚡ {msg} {bar:28.cyan/blue} {human_bytes:>9} / {human_total:>9} ({percent:>5.1}%) @ {binary_bytes_per_sec}/s ETA {eta}"
        )
        .unwrap()
        .with_key("human_bytes", |state: &ProgressState, w: &mut dyn std::fmt::Write| {
            let _ = write!(w, "{}", HumanBytes(state.pos()));
        })
        .with_key("human_total", |state: &ProgressState, w: &mut dyn std::fmt::Write| {
            let value = state
                .len()
                .map(|len| HumanBytes(len).to_string())
                .unwrap_or_else(|| "?".to_string());
            let _ = write!(w, "{}", value);
        })
    }

    fn format_label(raw: &str) -> String {
        const MAX_LABEL: usize = 40;
        let mut buf = String::new();
        for (idx, ch) in raw.chars().enumerate() {
            if idx >= MAX_LABEL - 1 {
                buf.push('…');
                return buf;
            }
            buf.push(ch);
        }
        buf
    }
}

struct PlainProgress {
    start: Instant,
    last_emit: Instant,
    last_line_len: usize,
    printed: bool,
}

impl PlainProgress {
    fn new() -> Self {
        Self {
            start: Instant::now(),
            last_emit: Instant::now(),
            last_line_len: 0,
            printed: false,
        }
    }

    fn update(&mut self, label: Option<&str>, downloaded: u64, total: u64) {
        const MIN_INTERVAL: Duration = Duration::from_millis(250);
        let now = Instant::now();
        if downloaded < total && now.duration_since(self.last_emit) < MIN_INTERVAL {
            return;
        }

        self.last_emit = now;
        let elapsed = now.duration_since(self.start).as_secs_f64().max(0.001);
        let speed = downloaded as f64 / elapsed; // bytes/sec
        let speed_mib = speed / (1024.0 * 1024.0);

        let (down_div, down_unit) = pick_display_unit(downloaded);
        let (total_div, total_unit) = pick_display_unit(total);

        let downloaded_val = downloaded as f64 / down_div;
        let total_val = total as f64 / total_div;

        let downloaded_str = format_scaled(downloaded_val, down_unit, downloaded);
        let total_str = format_scaled(total_val, total_unit, total);

        let percent = if total > 0 {
            (downloaded as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        let mut line = String::from("⚡ Fast download");
        if let Some(name) = label.filter(|name| !name.is_empty()) {
            line.push_str(&format!(" [{name}]"));
        }
        if total > 0 {
            if downloaded == 0 {
                line.push_str(&format!(": Preparing... ({total_str} {total_unit})"));
            } else {
                line.push_str(&format!(
                    ": {downloaded_str} {down_unit} / {total_str} {total_unit} ({percent:5.1}%) @ {speed_mib:5.1} MiB/s"
                ));
            }
        } else {
            line.push_str(&format!(": {downloaded_str} {down_unit} downloaded"));
        }

        let pad = self.last_line_len.saturating_sub(line.len());
        print!("\r{line}");
        if pad > 0 {
            for _ in 0..pad {
                print!(" ");
            }
        }
        io::stdout().flush().ok();

        self.last_line_len = line.len();
        self.printed = true;
    }

    fn finish(&mut self) {
        if self.printed {
            println!();
            self.printed = false;
            self.last_line_len = 0;
        }
    }
}

const KIB: u64 = 1024;
const MIB: u64 = KIB * 1024;
const GIB: u64 = MIB * 1024;

fn pick_display_unit(reference: u64) -> (f64, &'static str) {
    if reference >= GIB {
        (GIB as f64, "GiB")
    } else if reference >= MIB {
        (MIB as f64, "MiB")
    } else if reference >= KIB {
        (KIB as f64, "KiB")
    } else {
        (1.0, "B")
    }
}

fn format_scaled(value: f64, unit: &str, raw: u64) -> String {
    if unit == "B" {
        return raw.to_string();
    }

    if value >= 100.0 {
        format!("{value:6.1}")
    } else if value >= 10.0 {
        format!("{value:5.2}")
    } else if value >= 1.0 {
        format!("{value:4.2}")
    } else {
        format!("{value:.3}")
    }
}

impl PythonHelper {
    async fn prepare() -> Result<Self> {
        let env_dir = fast_env_dir()?;
        let script_path = helper_script_path()?;

        if let Some(parent) = env_dir.parent() {
            fs::create_dir_all(parent)
                .with_context(|| "Failed to create .conda parent directory")?;
        }
        if let Some(parent) = script_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| "Failed to create helper script directory")?;
        }

        let helper = Self {
            env_dir,
            script_path,
        };
        helper.write_script()?;
        helper.ensure_env().await?;
        Ok(helper)
    }

    async fn ensure_env(&self) -> Result<()> {
        if !self.python_path().exists() {
            self.create_env().await?;
        }

        if !self.marker_is_fresh()? {
            self.install_requirements().await?;
            self.write_marker()?;
        }

        Ok(())
    }

    async fn run(&self, request: &FastDownloadRequest<'_>) -> Result<()> {
        let python = self.python_path();
        let mut cmd = Command::new(&python);
        cmd.arg(&self.script_path)
            .arg("--repo-id")
            .arg(request.repo_id)
            .arg("--revision")
            .arg(request.revision)
            .arg("--repo-type")
            .arg(request.repo_type)
            .arg("--dest")
            .arg(request.destination)
            .kill_on_drop(true)
            .env("PYTHONUNBUFFERED", "1")
            .env("HF_HUB_DISABLE_TELEMETRY", "1");

        if let Some(token) = request.token {
            cmd.arg("--token").arg(token);
        }
        if request.force {
            cmd.arg("--force");
        }

        for file in request.files {
            cmd.arg("--file").arg(file);
        }

        let mut child = cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .with_context(|| "Failed to spawn fast-path downloader")?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Missing stdout from fast downloader"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("Missing stderr from fast downloader"))?;

        let mut lines = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr);
        let stderr_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            let _ = stderr_reader.read_to_end(&mut buf).await;
            buf
        });

        let mut cli_progress = if request.progress.is_none() {
            Some(CliProgressPrinter::new())
        } else {
            None
        };
        let mut ctrl_c = Box::pin(signal::ctrl_c());

        loop {
            tokio::select! {
                _ = &mut ctrl_c => {
                    let _ = child.kill().await;
                    finish_progress(&mut cli_progress);
                    bail!("fast download cancelled by user");
                }
                line = lines.next_line() => {
                    let line = line?;
                    let Some(line) = line else { break; };
                    if line.trim().is_empty() {
                        continue;
                    }
                    // Debug: print raw line to see what we get
                    // println!("[fast-path-raw] {}", line);
                    match serde_json::from_str::<EventEnvelope>(&line) {
                        Ok(event) => match event.event.as_str() {
                            "progress" => {
                                if let Some(cb) = request.progress {
                                    let downloaded = event.downloaded.unwrap_or(0);
                                    let total = event.total.unwrap_or(0);
                                    cb(downloaded, total);
                                } else if let Some(printer) = cli_progress.as_mut() {
                                    let downloaded = event.downloaded.unwrap_or(0);
                                    let total = event.total.unwrap_or(0);
                                    printer.update(event.file.as_deref(), downloaded, total);
                                }
                            }
                            "unavailable" => {
                                let reason = event
                                    .detail
                                    .or(event.reason)
                                    .unwrap_or_else(|| "fast helper unavailable".to_string());
                                let _ = child.kill().await;
                                finish_progress(&mut cli_progress);
                                bail!(reason);
                            }
                            "file-error" | "error" => {
                                let msg = event
                                    .message
                                    .unwrap_or_else(|| "fast helper reported an error".to_string());
                                let _ = child.kill().await;
                                finish_progress(&mut cli_progress);
                                return Err(anyhow!(msg));
                            }
                            _ => {}
                        },
                        Err(_) => {
                            finish_progress(&mut cli_progress);
                            println!("[fast-path] {line}");
                        }
                    }
                }
            }
        }

        let status = child.wait().await?;
        let stderr_buf: Vec<u8> = stderr_task.await.unwrap_or_default();
        let stderr_text = String::from_utf8_lossy(&stderr_buf).trim().to_string();

        if !status.success() {
            let reason = if stderr_text.is_empty() {
                format!("fast downloader exited with status {status}")
            } else {
                format!("fast downloader failed: {stderr_text}")
            };
            finish_progress(&mut cli_progress);
            bail!(reason);
        }

        Ok(())
    }

    fn python_path(&self) -> PathBuf {
        if cfg!(windows) {
            self.env_dir.join("Scripts").join("python.exe")
        } else {
            let bin = self.env_dir.join("bin");
            let python3 = bin.join("python3");
            if python3.exists() {
                python3
            } else {
                bin.join("python")
            }
        }
    }

    fn write_script(&self) -> Result<()> {
        if let Some(parent) = self.script_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create {}", parent.display()))?;
        }
        fs::write(&self.script_path, PY_HELPER_SOURCE).with_context(|| {
            format!(
                "Failed to write helper script at {}",
                self.script_path.display()
            )
        })?;
        #[cfg(unix)]
        {
            let mut perms = fs::metadata(&self.script_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&self.script_path, perms)?;
        }
        Ok(())
    }

    async fn create_env(&self) -> Result<()> {
        let bootstrap = find_bootstrap_python()?;
        println!(
            "ℹ️  Creating Python environment for fast downloads at {}...",
            self.env_dir.display()
        );
        let status = Command::new(&bootstrap)
            .arg("-m")
            .arg("venv")
            .arg(&self.env_dir)
            .status()
            .await
            .context("Failed to create Python venv")?;
        if !status.success() {
            bail!("python -m venv exited with {status}");
        }
        Ok(())
    }

    async fn install_requirements(&self) -> Result<()> {
        println!("ℹ️  Installing fast download dependencies...");
        let python = self.python_path();
        run_python_command(&python, &["-m", "pip", "install", "--upgrade", "pip"]).await?;
        let mut args = vec!["-m", "pip", "install", "--upgrade"];
        args.extend(PY_REQUIREMENTS);
        run_python_command(&python, &args).await?;
        Ok(())
    }

    fn marker_is_fresh(&self) -> Result<bool> {
        let marker_path = self.env_dir.join(ENV_MARKER_NAME);
        if !marker_path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(&marker_path)?;
        let marker: EnvMarker = match serde_json::from_str(&content) {
            Ok(marker) => marker,
            Err(_) => return Ok(false),
        };
        Ok(marker.matches())
    }

    fn write_marker(&self) -> Result<()> {
        let marker = EnvMarker::current();
        let marker_path = self.env_dir.join(ENV_MARKER_NAME);
        fs::write(&marker_path, serde_json::to_string_pretty(&marker)?)
            .with_context(|| format!("Failed to write env marker at {}", marker_path.display()))
    }
}

#[derive(Deserialize, Serialize)]
struct EnvMarker {
    helper_version: String,
    requirements: Vec<String>,
}

impl EnvMarker {
    fn current() -> Self {
        Self {
            helper_version: env!("CARGO_PKG_VERSION").to_string(),
            requirements: PY_REQUIREMENTS.iter().map(|r| r.to_string()).collect(),
        }
    }

    fn matches(&self) -> bool {
        self.helper_version == env!("CARGO_PKG_VERSION")
            && self.requirements
                == PY_REQUIREMENTS
                    .iter()
                    .map(|r| r.to_string())
                    .collect::<Vec<_>>()
    }
}

#[derive(Deserialize)]
struct EventEnvelope {
    event: String,
    file: Option<String>,
    downloaded: Option<u64>,
    total: Option<u64>,
    message: Option<String>,
    reason: Option<String>,
    detail: Option<String>,
}

async fn run_python_command(python: &Path, args: &[&str]) -> Result<()> {
    let mut cmd = Command::new(python);
    cmd.args(args);
    let status = cmd.status().await?;
    if !status.success() {
        bail!(
            "Command {:?} {:?} failed with status {}",
            python,
            args,
            status
        );
    }
    Ok(())
}

fn find_bootstrap_python() -> Result<PathBuf> {
    for candidate in PYTHON_CANDIDATES {
        if let Ok(path) = which::which(candidate) {
            return Ok(path);
        }
    }
    bail!("python3/python not found in PATH");
}

fn fast_env_dir() -> Result<PathBuf> {
    let data_root = get_data_root()?;
    let env_dir = data_root.join(".conda").join("gglib-hf-xet");
    if let Some(parent) = env_dir.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    Ok(env_dir)
}

fn helper_script_path() -> Result<PathBuf> {
    let data_root = get_data_root()?;
    let dir = data_root.join(".gglib-runtime").join("python");
    fs::create_dir_all(&dir)?;
    Ok(dir.join("hf_xet_downloader.py"))
}
