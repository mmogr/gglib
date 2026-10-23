//! The last few KiB an MCP server printed on stderr.
//!
//! The server's stderr is read to its end as it is written, so a server that
//! prints more than a pipe holds never blocks on the write, and what it
//! printed last is at hand when it exits: a server that cannot start says
//! why there, and nowhere else.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio::sync::watch;
use tokio::task::AbortHandle;

/// How many of the last bytes printed are kept.
pub(crate) const TAIL_BYTES: usize = 4096;

/// The tail of one server's stderr, filled while the server runs.
///
/// When a task reads it (`drain`), dropping the tail stops the read: a
/// grandchild (a server started through `npx`) can hold the pipe open after
/// the child is killed. When a thread of its own reads it (`drain_on_thread`,
/// how Windows reads it), the read outlives the drop until every writer has
/// closed the pipe, and the process exits without waiting for it.
pub(crate) struct StderrTail {
    kept: Arc<Mutex<Kept>>,
    /// Closed, never sent on, when the read ends.
    ended: watch::Receiver<()>,
    /// The task reading, aborted on drop; none for a read on a thread.
    reader: Option<AbortHandle>,
}

impl StderrTail {
    /// Read `stream` to its end in a spawned task, keeping its last
    /// [`TAIL_BYTES`].
    ///
    /// Not for a Windows pipe: tokio reads one on a blocking thread that an
    /// abort cannot stop and that the runtime waits for when it shuts down,
    /// so a grandchild holding the pipe would hold the process's exit too.
    #[cfg(any(not(windows), test))]
    pub(crate) fn drain<R>(mut stream: R) -> Self
    where
        R: tokio::io::AsyncRead + Send + Unpin + 'static,
    {
        use tokio::io::AsyncReadExt as _;

        let kept = Arc::new(Mutex::new(Kept::default()));
        let (ending, ended) = watch::channel(());
        let filling = Arc::clone(&kept);
        let reader = tokio::spawn(async move {
            // Held until the read ends; dropping it is what `ended` sees.
            let _ending = ending;
            let mut chunk = [0_u8; 1024];
            while let Ok(read @ 1..) = stream.read(&mut chunk).await {
                filling
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(&chunk[..read]);
            }
        })
        .abort_handle();
        Self {
            kept,
            ended,
            reader: Some(reader),
        }
    }

    /// Read `stream` to its end on a detached thread of its own, keeping its
    /// last [`TAIL_BYTES`]: the read runs until every writer has closed the
    /// stream, and the process exits without waiting for it.
    ///
    /// # Errors
    ///
    /// When the thread cannot be started.
    #[cfg(any(windows, test))]
    pub(crate) fn drain_on_thread<R>(mut stream: R) -> std::io::Result<Self>
    where
        R: std::io::Read + Send + 'static,
    {
        let kept = Arc::new(Mutex::new(Kept::default()));
        let (ending, ended) = watch::channel(());
        let filling = Arc::clone(&kept);
        std::thread::Builder::new()
            .name("mcp-stderr".to_owned())
            .spawn(move || {
                // Held until the read ends; dropping it is what `ended` sees.
                let _ending = ending;
                let mut chunk = [0_u8; 1024];
                loop {
                    match stream.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(read) => filling
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .push(&chunk[..read]),
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                        Err(_) => break,
                    }
                }
            })?;
        Ok(Self {
            kept,
            ended,
            reader: None,
        })
    }

    /// What the server printed, once its stderr has ended or `wait` has
    /// passed, whichever is first; see [`Kept::text`].
    pub(crate) async fn settled(&self, wait: Duration) -> String {
        let mut ended = self.ended.clone();
        // `changed` returns once the sender is gone: the read has ended.
        let _ = tokio::time::timeout(wait, ended.changed()).await;
        self.kept
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .text()
    }
}

impl Drop for StderrTail {
    fn drop(&mut self) {
        if let Some(reader) = &self.reader {
            reader.abort();
        }
    }
}

/// The bytes kept, and whether earlier ones were let go.
#[derive(Default)]
struct Kept {
    bytes: VecDeque<u8>,
    cut: bool,
}

impl Kept {
    /// Append `chunk`, letting the oldest bytes go past [`TAIL_BYTES`].
    fn push(&mut self, chunk: &[u8]) {
        self.bytes.extend(chunk);
        let over = self.bytes.len().saturating_sub(TAIL_BYTES);
        if over > 0 {
            self.bytes.drain(..over);
            self.cut = true;
        }
    }

    /// The bytes kept as one line of text: each line trimmed, a control
    /// character or one that reorders text (a bidirectional override or
    /// isolate) made a space, blank lines dropped, the rest joined by
    /// ` | `, led by `…` when earlier bytes were let go. A character the cut
    /// split is dropped whole, and bytes that are not UTF-8 are replaced, so
    /// the text is always valid. Empty when the server printed nothing.
    fn text(&mut self) -> String {
        let mut bytes: &[u8] = self.bytes.make_contiguous();
        if self.cut {
            // A UTF-8 continuation byte is `10xxxxxx`; a character is at most
            // four bytes, so at most three of them lead.
            let split = bytes
                .iter()
                .take(3)
                .take_while(|&&byte| byte & 0xC0 == 0x80)
                .count();
            bytes = &bytes[split..];
        }
        let lines: Vec<String> = String::from_utf8_lossy(bytes)
            .lines()
            .map(|line| {
                line.chars()
                    .map(|c| {
                        if c.is_control() || reorders(c) {
                            ' '
                        } else {
                            c
                        }
                    })
                    .collect::<String>()
            })
            .map(|line| line.trim().to_owned())
            .filter(|line| !line.is_empty())
            .collect();
        if lines.is_empty() {
            return String::new();
        }
        let text = lines.join(" | ");
        if self.cut { format!("…{text}") } else { text }
    }
}

/// Whether `c` changes the order text displays in: the bidirectional marks,
/// embeddings, overrides and isolates, which a line quoting a server would
/// otherwise let the server rearrange.
const fn reorders(c: char) -> bool {
    matches!(
        c,
        '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
    )
}

#[cfg(test)]
#[path = "stderr_tail_tests.rs"]
mod stderr_tail_tests;
