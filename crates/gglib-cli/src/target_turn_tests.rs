//! Tests for [`super`]: what a turn's model resolves to on the paired
//! machine, what goes on the wire for it, and what remembering it writes to
//! a settings store that another writer shares.

use std::sync::Mutex;

use async_trait::async_trait;
use gglib_core::RemotePairing;
use gglib_core::domain::ModelDetailDto;
use gglib_core::ports::RepositoryError;

use super::*;

// ── Resolving on the paired machine ──────────────────────────────────────

/// What the far detail route answers: model `id`, called `name` there, and
/// the profile the identifier named.
fn found(id: i64, name: &str, profile: Option<&str>) -> ModelLookup {
    let detail: ModelDetailDto = serde_json::from_value(serde_json::json!({
        "id": id,
        "name": name,
        "paramCountB": 8.0,
        "addedAt": "2026-10-01 12:00:00",
        "metadata": {},
    }))
    .unwrap();
    ModelLookup {
        profile: profile.map(str::to_owned),
        detail,
    }
}

/// A turn on the paired machine, resolved against a lookup that answers
/// with `answer` and records what it was asked.
async fn far_turn(typed: &str, answer: ModelLookup) -> (TurnModel, Vec<String>) {
    let asked = Mutex::new(Vec::new());
    let turn = resolve_far(typed, "fp-desk".to_owned(), "desk".to_owned(), async |id| {
        asked.lock().unwrap().push(id.to_owned());
        Ok(answer)
    })
    .await
    .expect("the model resolves");
    (turn, asked.into_inner().unwrap())
}

/// `gglib chat 3 --remote`: the far machine is asked for `3` once, and the
/// turn sends `"3"`.
#[tokio::test]
async fn an_id_is_sent_as_the_id() {
    let (turn, asked) = far_turn("3", found(3, "qwen3", None)).await;

    assert_eq!(asked, ["3"], "one lookup, of what was typed");
    assert_eq!(
        Target::Remote.wire_model_name(None, &turn.identifier),
        Some("3".to_owned())
    );
    assert_eq!(
        turn.model_ref,
        Some(ModelRef {
            machine: Machine::Paired {
                fingerprint: "fp-desk".to_owned()
            },
            id: 3,
        })
    );
    assert_eq!(turn.shown(), "qwen3 (3) on desk");
}

/// `gglib chat qwen3:coding --remote`: the name and its profile are resolved
/// there, and the turn sends the model's id with the profile, so a second
/// model of the same name cannot answer it.
#[tokio::test]
async fn a_name_with_a_profile_is_sent_as_its_id_with_the_profile() {
    let (turn, asked) = far_turn("qwen3:coding", found(7, "qwen3", Some("coding"))).await;

    assert_eq!(asked, ["qwen3:coding"]);
    assert_eq!(
        Target::Remote.wire_model_name(None, &turn.identifier),
        Some("7:coding".to_owned())
    );
    assert_eq!(turn.far_profile.as_deref(), Some("coding"));
    assert_eq!(turn.name, "qwen3");
    assert_eq!(turn.shown(), "qwen3 (7:coding) on desk");
}

/// A model the far machine does not have is refused here, before a turn
/// starts, naming what was asked for and where.
#[tokio::test]
async fn a_model_the_far_machine_lacks_is_refused_before_the_turn() {
    let refused = resolve_far("9", "fp".to_owned(), "desk".to_owned(), async |_| {
        Err(anyhow!("daemon answered 404 Not Found: no model 9"))
    })
    .await
    .expect_err("refused");

    assert_eq!(refused.to_string(), "looking up '9' on desk");
    assert!(format!("{refused:#}").contains("404"), "{refused:#}");
}

/// A turn here keeps what was typed, and names the catalogue entry by id
/// when there is one.
#[test]
fn a_turn_here_names_its_catalogue_entry_by_id() {
    let unknown = TurnModel::here("external".to_owned(), None);
    assert_eq!(unknown.model_ref, None);
    assert_eq!(unknown.shown(), "external");
    assert_eq!(far_wire(3, None), "3");
    assert_eq!(far_wire(3, Some("coding")), "3:coding");
}

/// A saved turn's replies name its model as the machine that serves it has
/// it: the catalogue's name and quantisation here, the far machine's there,
/// and what was typed for a server this catalogue does not hold. Never a
/// device, nor a context, which only the daemon knows.
#[tokio::test]
async fn a_turns_replies_are_named_for_its_model_and_its_quantisation() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = crate::bootstrap::test_context(dir.path()).await;
    let path = dir.path().join("qwen3.gguf");
    let mut entry =
        gglib_core::domain::NewModel::new("qwen3".to_owned(), path, 8.0, chrono::Utc::now());
    entry.quantization = Some("Q4_K_M".to_owned());
    let id = ctx.app.models().add(entry).await.expect("registered").id;
    let here = Target::Local.resolve_turn(&ctx, id.to_string()).await;
    let mut there = found(3, "far-qwen", None);
    there.detail.quantization = Some("Q8_0".to_owned());
    let (there, _) = far_turn("3", there).await;
    let typed = TurnModel::here("external".to_owned(), None);

    let named = |turn: &TurnModel| {
        let made_by = turn.made_by();
        assert_eq!((made_by.device, made_by.context_size), (None, None));
        (made_by.model, made_by.quantization)
    };
    let quantised = |name: &str, q: &str| (name.to_owned(), Some(q.to_owned()));
    assert_eq!(
        named(&here.expect("resolved")),
        quantised("qwen3", "Q4_K_M")
    );
    assert_eq!(named(&there), quantised("far-qwen", "Q8_0"));
    assert_eq!(named(&typed), ("external".to_owned(), None));
}

// ── Remembering the model ────────────────────────────────────────────────

/// Machine A's ticket, the minimal v0 vector, and the fingerprint it
/// carries: the machine a turn resolves its model on.
const TICKET_A: &str = "pipeadlvvgabqkyqvn6vjp7nhslea45a5yls6pnkmizfv4bbu2hxa5iruaaauhlp2na";
const FINGERPRINT_A: &str = "d75a980182b1";
/// A second machine, over another public key.
const TICKET_B: &str = "pipeaa6uaf6d5bbyswusw4fkoti3p26jzgbmz4xmjfumydgvl4jk6rtayaaa2e4g6hq";

/// A write that lands between a turn's read of the settings and its write.
type Between = Box<dyn FnOnce(&mut Settings) + Send>;

/// A settings store another writer shares: the first `load` answers with
/// the settings as they stood, and then `between` lands, the way a write
/// from the daemon would while the turn is under way.
struct Shared {
    stored: Mutex<Settings>,
    between: Mutex<Option<Between>>,
}

impl Shared {
    fn paired_with_a(between: impl FnOnce(&mut Settings) + Send + 'static) -> Self {
        Self::paired_with(TICKET_A, "key-a", between)
    }

    fn paired_with(
        ticket: &str,
        api_key: &str,
        between: impl FnOnce(&mut Settings) + Send + 'static,
    ) -> Self {
        let mut settings = Settings::with_defaults();
        settings.remote_pairing = pairing(ticket, api_key);
        Self {
            stored: Mutex::new(settings),
            between: Mutex::new(Some(Box::new(between))),
        }
    }

    fn pairing(&self) -> Option<RemotePairing> {
        self.stored.lock().unwrap().remote_pairing.clone()
    }
}

#[async_trait]
impl SettingsRepository for Shared {
    #[allow(
        clippy::significant_drop_in_scrutinee,
        clippy::significant_drop_tightening,
        reason = "grandfathered at lint inheritance, #1157"
    )]
    async fn load(&self) -> Result<Settings, RepositoryError> {
        let mut stored = self.stored.lock().unwrap();
        let read = stored.clone();
        if let Some(write) = self.between.lock().unwrap().take() {
            write(&mut stored);
        }
        Ok(read)
    }

    async fn save(&self, settings: &Settings) -> Result<(), RepositoryError> {
        *self.stored.lock().unwrap() = settings.clone();
        Ok(())
    }
}

#[allow(
    clippy::unnecessary_wraps,
    reason = "grandfathered at lint inheritance, #1157"
)]
fn pairing(ticket: &str, api_key: &str) -> Option<RemotePairing> {
    Some(RemotePairing {
        ticket: ticket.to_owned(),
        api_key: api_key.to_owned(),
        default_model: None,
        port: Some(8180),
        name: None,
    })
}

/// A turn's resolved model is remembered on the pairing it was resolved
/// for, in the form the turn sent.
#[tokio::test]
async fn a_turns_model_is_remembered_on_its_pairing_as_sent() {
    let store = Shared::paired_with_a(|_| {});

    remember(&store, FINGERPRINT_A, "3:coding")
        .await
        .expect("remembered");

    let stored = store.pairing().expect("the pairing is still stored");
    assert_eq!(stored.default_model.as_deref(), Some("3:coding"));
    assert_eq!(stored.api_key, "key-a", "remembering touched the key");
}

/// Naming the model reads the memory and writes nothing: what is
/// remembered is what the far machine resolved, not what was typed.
#[tokio::test]
async fn naming_a_model_writes_nothing_until_it_resolves() {
    let store = Shared::paired_with_a(|_| {});

    let model = remembered_model(&store, "qwen3".to_owned()).await;

    assert_eq!(model.expect("a model was named"), "qwen3");
    assert_eq!(store.pairing(), pairing(TICKET_A, "key-a"));
}

/// Remembering a model keeps the key a re-pair wrote while the turn ran.
#[tokio::test]
async fn remembering_a_model_keeps_the_key_a_re_pair_wrote() {
    let store = Shared::paired_with_a(|now| now.remote_pairing = pairing(TICKET_A, "key-a-again"));

    remember(&store, FINGERPRINT_A, "3")
        .await
        .expect("remembered");

    let stored = store.pairing().expect("the pairing is still stored");
    assert_eq!(
        stored.api_key, "key-a-again",
        "the re-pair's key was put back"
    );
    assert_eq!(stored.default_model.as_deref(), Some("3"));
}

/// A model resolved on one machine is not remembered on a pairing with
/// another, which a join wrote while the turn ran: after remembering read
/// the settings, or before, while the far machine was being asked.
#[tokio::test]
async fn a_model_is_not_remembered_for_another_machine() {
    let during_the_write =
        Shared::paired_with_a(|now| now.remote_pairing = pairing(TICKET_B, "key-b"));
    let during_the_lookup = Shared::paired_with(TICKET_B, "key-b", |_| {});

    for store in [during_the_write, during_the_lookup] {
        remember(&store, FINGERPRINT_A, "3")
            .await
            .expect("nothing to write");

        assert_eq!(
            store.pairing(),
            pairing(TICKET_B, "key-b"),
            "the other machine's pairing was changed"
        );
    }
}

/// A pairing forgotten while the turn ran stays forgotten.
#[tokio::test]
async fn a_forgotten_pairing_is_not_brought_back() {
    let store = Shared::paired_with_a(|now| now.remote_pairing = None);

    remember(&store, FINGERPRINT_A, "3")
        .await
        .expect("nothing to write");

    assert_eq!(store.pairing(), None, "remembering a model brought it back");
}
