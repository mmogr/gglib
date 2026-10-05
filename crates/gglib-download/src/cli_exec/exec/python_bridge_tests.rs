//! Tests of the Python bridge.

use super::*;

#[tokio::test]
async fn only_the_end_of_stderr_is_kept() {
    let mut noise = vec![b'.'; 3 * STDERR_TAIL_BYTES];
    noise.extend_from_slice(b"the last line");

    let tail = read_tail(noise.as_slice(), STDERR_TAIL_BYTES).await;

    assert_eq!(tail.len(), STDERR_TAIL_BYTES);
    assert!(tail.ends_with(b"the last line"));
}

#[tokio::test]
async fn a_short_stderr_is_kept_whole() {
    let tail = read_tail(b"Traceback: boom".as_slice(), STDERR_TAIL_BYTES).await;

    assert_eq!(tail, b"Traceback: boom");
}

/// A request for one file, with `token` and reporting to `progress`.
fn request(
    token: Option<&'static str>,
    progress: Option<RawCallback>,
) -> FastDownloadRequest<'static> {
    FastDownloadRequest {
        repo_id: "owner/repo",
        revision: "main",
        repo_type: "model",
        destination: Path::new("/nowhere"),
        file: "model.gguf",
        token,
        force: false,
        progress,
        notice: None,
        cancel_token: None,
    }
}

/// The helper takes no token argument: it reads `HF_TOKEN`.
#[test]
fn the_token_goes_in_the_environment_and_not_the_arguments() {
    let request = request(Some("hf_secret"), None);

    let command = helper_command(Path::new("python"), Path::new("helper.py"), &request);
    let command = command.as_std();

    let token = command
        .get_envs()
        .find_map(|(name, value)| (name == "HF_TOKEN").then_some(value));
    assert_eq!(token, Some(Some(std::ffi::OsStr::new("hf_secret"))));
    let arguments: Vec<_> = command.get_args().map(|a| a.to_string_lossy()).collect();
    assert!(
        !arguments
            .iter()
            .any(|a| a.contains("hf_secret") || a == "--token"),
        "{arguments:?}"
    );
}

#[test]
fn a_progress_line_is_a_reading_and_a_zero_total_is_unknown() {
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let request = request(
        None,
        Some(Arc::new(move |raw| sink.lock().unwrap().push(raw))),
    );

    for total in [0, 900] {
        let event = PythonEvent::Progress {
            written: 100,
            received: 300,
            total,
        };
        handle_event(event, &request).expect("a progress line is not an error");
    }

    assert_eq!(
        *seen.lock().unwrap(),
        [
            RawProgress::new(100, 300, None),
            RawProgress::new(100, 300, Some(900))
        ]
    );
}
