//! What `config sd prompt` says and does in each case, with a stand-in for
//! the install: nothing here downloads or builds.

use std::sync::atomic::{AtomicU32, Ordering};

use anyhow::anyhow;

use super::*;

/// What one offer came to.
struct Offered {
    /// What it returned: `make setup` goes on only after an `Ok`.
    result: Result<()>,
    /// Everything it wrote.
    said: String,
    /// How many times it ran the install.
    installs: u32,
}

/// The offer with `typed` on its input, and an install that counts its runs
/// and ends as `install_fails` says.
async fn offered(
    installed: bool,
    terminal: bool,
    typed: &str,
    install_fails: Option<&str>,
) -> Offered {
    let installs = AtomicU32::new(0);
    let mut out = Vec::new();
    let result = offer(installed, terminal, &mut typed.as_bytes(), &mut out, || {
        installs.fetch_add(1, Ordering::Relaxed);
        async move { install_fails.map_or(Ok(()), |why| Err(anyhow!("{why}"))) }
    })
    .await;
    Offered {
        result,
        said: String::from_utf8(out).expect("text"),
        installs: installs.load(Ordering::Relaxed),
    }
}

const QUESTION: &str = "Install the image runtime now? (y/N): ";

/// Even a yes waiting on the input is not read: nothing is asked.
#[tokio::test]
async fn an_installed_runtime_is_said_to_be_and_nothing_is_asked() {
    let offered = offered(true, true, "y\n", None).await;

    assert!(offered.result.is_ok());
    assert_eq!(
        offered.said,
        format!("{SUCCESS}\u{2713} The image runtime is already installed{RESET}\n")
    );
    assert_eq!(offered.installs, 0);
}

/// A yes on a pipe is nobody's answer.
#[tokio::test]
async fn with_no_terminal_it_skips_with_one_line_and_installs_nothing() {
    let offered = offered(false, false, "y\n", None).await;

    assert!(offered.result.is_ok());
    assert_eq!(
        offered.said,
        format!(
            "{INFO}The image runtime is not installed. To draw pictures with an image model, \
             run:{RESET} gglib config sd install\n"
        )
    );
    assert_eq!(offered.installs, 0);
}

#[tokio::test]
async fn a_yes_is_told_what_it_is_and_installs_it_once() {
    let offered = offered(false, true, "y\n", None).await;

    assert!(offered.result.is_ok());
    assert_eq!(offered.installs, 1);
    assert_eq!(
        offered.said,
        format!(
            "\n{BOLD}Image runtime{RESET} \u{2014} stable-diffusion.cpp's sd-server, which \
             draws pictures with an image model.\n  \
             Optional: chat models work without it. Installing downloads its pre-built release\n  \
             for this machine, or builds it from source where there is none.\n\n\
             {QUESTION}\n"
        )
    );
}

/// The image runtime is optional, so setup goes on: the offer returns `Ok`
/// and says how to try again.
#[tokio::test]
async fn a_failed_install_is_a_skipped_step_that_says_how_to_retry() {
    let offered = offered(false, true, "yes\n", Some("no route to host")).await;

    assert!(offered.result.is_ok(), "{:?}", offered.result);
    assert_eq!(offered.installs, 1);
    let after = offered.said.split_once(QUESTION).expect("it asked first").1;
    assert_eq!(
        after,
        format!(
            "\n{WARNING}\u{25cb} Could not install the image runtime:{RESET} no route to host\n  \
             Skipped. Chat models work without it.\n  \
             Retry with `gglib config sd install`, or in Settings \u{2192} System \u{2192} \
             Image runtime.\n"
        )
    );
}

/// Enter is no, and so is the end of input.
#[tokio::test]
async fn a_no_or_enter_installs_nothing_and_says_how_to_install_later() {
    for typed in ["n\n", "no\n", "\n", ""] {
        let offered = offered(false, true, typed, None).await;

        assert!(offered.result.is_ok(), "{typed:?}");
        assert_eq!(offered.installs, 0, "{typed:?}");
        let after = offered.said.split_once(QUESTION).expect("it asked first").1;
        assert_eq!(
            after,
            format!(
                "\n{INFO}Skipped the image runtime.{RESET}\n  \
                 Install it later with `gglib config sd install`, or in Settings \u{2192} \
                 System \u{2192} Image runtime.\n"
            ),
            "{typed:?}"
        );
    }
}
