//! The Hub's rate limit on the wire: a stand-in Hub on the loopback answers
//! with the refusal huggingface.co sent on 2026-11-23, and then as it
//! answers when the limit has lifted. A test where nothing waits the limit
//! out keeps the recorded reset, ten seconds, so a slow run cannot reach it;
//! one that waits shortens it to a second or two.

use std::io::Write as _;
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use gglib_core::hub_limit::{HubBucket, HubGate, LIMITED_IN_A_ROW, OnLimit};
use gglib_core::ports::huggingface::{HfClientPort, HfPortError};

use super::Waiting;
use crate::client::DefaultHfClient;
use crate::config::HfClientConfig;

/// The repository every test asks about.
const REPO: &str = "owner/repo";

/// The recorded refusal's message, for the documentation address it names.
const REFUSED: &str = "We had to rate limit your IP (203.0.113.7). To continue using our service, create a HF account or login to your existing account, and make sure you pass a HF_TOKEN if you're using the API.";

/// The recorded reset, in seconds.
const RECORDED_RESET: u64 = 10;

/// The recorded refusal of the API, its reset `seconds` away.
fn refusal(seconds: u64) -> String {
    let body = format!("{{\"error\":\"{REFUSED}\",\"retryAfter\":{seconds}}}");
    format!(
        "HTTP/1.1 429 Too Many Requests\r\n\
         content-type: application/json; charset=utf-8\r\n\
         content-length: {}\r\n\
         ratelimit: \"api\";r=0;t={seconds}\r\n\
         ratelimit-policy: \"fixed window\";\"api\";q=500;w=300\r\n\
         retry-after: {seconds}\r\n\
         x-error-message: {REFUSED}\r\n\
         connection: close\r\n\r\n{body}",
        body.len()
    )
}

/// The repository's model info, with `ratelimit` as its limit header.
fn answer(ratelimit: &str) -> String {
    let body = format!("{{\"id\":\"{REPO}\",\"downloads\":1,\"likes\":1,\"siblings\":[]}}");
    format!(
        "HTTP/1.1 200 OK\r\n\
         content-type: application/json; charset=utf-8\r\n\
         content-length: {}\r\n\
         ratelimit: {ratelimit}\r\n\
         ratelimit-policy: \"fixed window\";\"api\";q=500;w=300\r\n\
         connection: close\r\n\r\n{body}",
        body.len()
    )
}

/// An ordinary answer of the API, as recorded.
fn served() -> String {
    answer("\"api\";r=485;t=142")
}

/// A stand-in Hub: where it listens, how many requests it has been asked,
/// and the thread that answers them.
struct Hub {
    base_url: String,
    asked: Arc<AtomicUsize>,
    answering: JoinHandle<()>,
}

impl Hub {
    /// A Hub that answers its requests with `answers`, one each, in order,
    /// and then stops listening.
    fn answering(answers: Vec<String>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let base_url = format!("http://{}/api/models", listener.local_addr().unwrap());
        let asked = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&asked);
        let answering = std::thread::spawn(move || {
            for answer in answers {
                // The whole request is read before the answer is written.
                let (_request, mut stream) =
                    gglib_test_http::blocking::accept(&listener).expect("a request");
                counted.fetch_add(1, Ordering::SeqCst);
                stream.write_all(answer.as_bytes()).expect("the answer");
            }
        });
        Self {
            base_url,
            asked,
            answering,
        }
    }

    /// How many requests have reached it.
    fn asked(&self) -> usize {
        self.asked.load(Ordering::SeqCst)
    }

    /// A client of this Hub that meets its limit as `on_limit` says and
    /// retries no failure of its own.
    fn client(&self, on_limit: OnLimit) -> DefaultHfClient {
        self.client_of(HfClientConfig::new().with_on_limit(on_limit))
    }

    /// A client of this Hub as `config` makes it, retrying no failure of
    /// its own.
    fn client_of(&self, config: HfClientConfig) -> DefaultHfClient {
        DefaultHfClient::new(&HfClientConfig {
            base_url: self.base_url.clone(),
            max_retries: 0,
            ..config
        })
    }
}

/// What a notice was told, in order.
#[derive(Clone, Default)]
struct Told(Arc<Mutex<Vec<Option<String>>>>);

impl Told {
    fn on_limit(&self) -> OnLimit {
        let told = Arc::clone(&self.0);
        OnLimit::Wait(Arc::new(move |line: Option<&str>| {
            told.lock().unwrap().push(line.map(str::to_owned));
        }))
    }

    fn lines(&self) -> Vec<Option<String>> {
        self.0.lock().unwrap().clone()
    }
}

/// A client that surfaces a limit fails with the allowance and the time it
/// lifts, and the request after it is refused without reaching the Hub.
/// (That the allowance opens at its reset is the gate's tests'.)
#[tokio::test]
async fn a_surfaced_refusal_carries_its_reset_and_closes_the_allowance() {
    let hub = Hub::answering(vec![refusal(RECORDED_RESET)]);
    let client = hub.client(OnLimit::Surface);

    let refused = HfClientPort::get_model_info(&client, REPO).await;
    let Err(HfPortError::RateLimited { bucket, resets_in }) = refused else {
        panic!("not a rate limit: {refused:?}");
    };
    assert_eq!(bucket, HubBucket::Api);
    assert!(
        resets_in <= Duration::from_secs(RECORDED_RESET),
        "{resets_in:?}"
    );
    assert!(!resets_in.is_zero());

    let again = HfClientPort::get_model_info(&client, REPO).await;
    let Err(HfPortError::RateLimited { bucket, .. }) = again else {
        panic!("not a rate limit: {again:?}");
    };
    assert_eq!(bucket, HubBucket::Api);
    assert_eq!(hub.asked(), 1, "a closed allowance was asked");
    hub.answering.join().unwrap();
}

/// A served answer that leaves none closes the allowance as a refusal
/// does: the next request would be refused.
#[tokio::test]
async fn an_answer_that_leaves_none_closes_the_allowance() {
    let hub = Hub::answering(vec![answer("\"api\";r=0;t=142")]);
    let client = hub.client(OnLimit::Surface);

    let info = HfClientPort::get_model_info(&client, REPO).await;
    assert_eq!(info.expect("the model info").model_id, REPO);
    let refused = HfClientPort::get_model_info(&client, REPO).await;

    assert!(
        matches!(
            refused,
            Err(HfPortError::RateLimited {
                bucket: HubBucket::Api,
                ..
            })
        ),
        "{refused:?}"
    );
    assert_eq!(hub.asked(), 1);
    hub.answering.join().unwrap();
}

/// A client that waits a limit out tells its notice the sentence, then that
/// the wait is over, and answers with what the Hub sent after the reset.
/// The refusal spends none of the client's retries: it has none.
#[tokio::test]
async fn a_waiting_client_waits_out_the_reset_and_asks_again() {
    let hub = Hub::answering(vec![refusal(2), served()]);
    let told = Told::default();
    let client = hub.client(told.on_limit());

    let info = HfClientPort::get_model_info(&client, REPO).await;

    assert_eq!(info.expect("the model info").model_id, REPO);
    assert_eq!(hub.asked(), 2);
    let lines = told.lines();
    assert!(
        lines
            .iter()
            .flatten()
            .any(|line| line.starts_with("waiting for Hugging Face's rate limit, 0:0")),
        "{lines:?}"
    );
    assert_eq!(lines.last(), Some(&None), "{lines:?}");
    assert_eq!(lines.iter().filter(|line| line.is_none()).count(), 1);
    hub.answering.join().unwrap();
}

/// A client refused again and again gives up, with the limit, after
/// [`LIMITED_IN_A_ROW`] refusals.
#[tokio::test]
async fn a_waiting_client_gives_up_after_refusals_in_a_row() {
    let refusals = usize::try_from(LIMITED_IN_A_ROW).unwrap();
    let hub = Hub::answering(vec![refusal(1); refusals]);
    let told = Told::default();
    let client = hub.client(told.on_limit());

    let refused = HfClientPort::get_model_info(&client, REPO).await;

    assert!(
        matches!(
            refused,
            Err(HfPortError::RateLimited {
                bucket: HubBucket::Api,
                ..
            })
        ),
        "{refused:?}"
    );
    assert_eq!(hub.asked(), refusals);
    hub.answering.join().unwrap();
}

/// Two clients that share a gate share a refusal: the second does not ask.
#[tokio::test]
async fn clients_that_share_a_gate_share_a_refusal() {
    let hub = Hub::answering(vec![refusal(RECORDED_RESET)]);
    let gate = Arc::new(HubGate::new());
    let first = hub.client_of(HfClientConfig::new().with_gate(Arc::clone(&gate)));
    let second = hub.client_of(HfClientConfig::new().with_gate(gate));

    let refused = HfClientPort::get_model_info(&first, REPO).await;
    let also = HfClientPort::get_model_info(&second, REPO).await;

    for limited in [&refused, &also] {
        assert!(
            matches!(
                limited,
                Err(HfPortError::RateLimited {
                    bucket: HubBucket::Api,
                    ..
                })
            ),
            "{limited:?}"
        );
    }
    assert_eq!(hub.asked(), 1);
    hub.answering.join().unwrap();
}

/// Of several requests waiting at once, as a search's listings do, the last
/// to stop waiting tells the notice the wait is over, and only it does.
#[test]
fn the_last_request_to_stop_waiting_ends_the_wait() {
    let told = Told::default();
    let OnLimit::Wait(notice) = told.on_limit() else {
        unreachable!("a notice is a wait")
    };
    let count = AtomicUsize::new(0);

    let first = Waiting::start(&count, &notice);
    let second = Waiting::start(&count, &notice);
    drop(first);
    assert!(told.lines().is_empty(), "{:?}", told.lines());
    drop(second);

    assert_eq!(told.lines(), [None]);
}
