//! `contracts/chats/branching.json`: the branching rules as cases. A plan
//! case is a chat's messages, whether a reply to it is being written, one
//! change, what the rules answer, and whether the chat is answerable; a
//! points case is a family of chats, the chat read, and its branch points.
//!
//! ggchat keeps its own chats on the phone and mirrors these rules in
//! Swift; it replays a byte-for-byte copy of this file, so the two cannot
//! drift. Each answer is recorded from the rules here.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::fixture::{family, greeted, image, kyoto, unanswered};
use super::{BranchPoint, ChatChange, LineChat, Plan, answerable, plan, points};
use crate::domain::attachment::{AttachmentId, AttachmentInfo};
use crate::domain::chat::{Message, MessageRole};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Recorded {
    about: String,
    plans: Vec<PlanCase>,
    points: Vec<PointsCase>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanCase {
    name: String,
    path: Vec<PathRow>,
    busy: bool,
    change: ChatChange,
    answer: Answer,
    answerable: bool,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PathRow {
    id: i64,
    role: MessageRole,
    content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    images: Vec<AttachmentId>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Answer {
    Plan(Plan),
    Refused(String),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PointsCase {
    name: String,
    me: i64,
    family: Vec<LineChat>,
    points: Vec<BranchPoint>,
}

fn path_rows(path: &[Message]) -> Vec<PathRow> {
    path.iter()
        .map(|m| PathRow {
            id: m.id,
            role: m.role,
            content: m.content.clone(),
            images: m.images.iter().map(|image| image.id.clone()).collect(),
        })
        .collect()
}

fn messages(rows: &[PathRow]) -> Vec<Message> {
    rows.iter()
        .map(|row| Message {
            id: row.id,
            conversation_id: 7,
            origin_id: None,
            role: row.role,
            content: row.content.clone(),
            created_at: String::new(),
            metadata: None,
            images: row
                .images
                .iter()
                .map(|id| AttachmentInfo {
                    id: id.clone(),
                    mime: "image/png".to_owned(),
                    width: 640,
                    height: 480,
                })
                .collect(),
        })
        .collect()
}

fn answer(path: &[Message], change: &ChatChange, busy: bool) -> Answer {
    plan(path, change, busy).map_or_else(
        |refused| Answer::Refused(refused.code().to_owned()),
        Answer::Plan,
    )
}

fn plan_case(name: &str, path: &[Message], busy: bool, change: ChatChange) -> PlanCase {
    PlanCase {
        name: name.to_owned(),
        path: path_rows(path),
        busy,
        answer: answer(path, &change, busy),
        answerable: answerable(path).is_ok(),
        change,
    }
}

fn points_case(name: &str, me: i64, chats: &[LineChat]) -> PointsCase {
    PointsCase {
        name: name.to_owned(),
        me,
        family: chats.to_vec(),
        points: points(me, chats),
    }
}

fn edit(message_id: i64, content: &str, images: Vec<AttachmentId>) -> ChatChange {
    ChatChange::Edit {
        message_id,
        content: content.to_owned(),
        images,
    }
}

fn plan_cases() -> Vec<PlanCase> {
    use ChatChange::{Branch, Regenerate};
    let (kyoto, unanswered, greeted) = (kyoto(), unanswered(), greeted());
    vec![
        plan_case(
            "an edit of an answered question branches before it",
            &kyoto,
            false,
            edit(3, "Make it longer", vec![]),
        ),
        plan_case(
            "an edit of the first question branches with nothing copied",
            &kyoto,
            false,
            edit(1, "Plan a trip to Osaka", vec![]),
        ),
        plan_case(
            "an edit of the last question nothing answers replaces it",
            &unanswered,
            false,
            edit(7, "And with kids?", vec![]),
        ),
        plan_case(
            "an edit of the last question while a reply is written branches",
            &unanswered,
            true,
            edit(7, "And with a dog?", vec![image(0)]),
        ),
        plan_case(
            "an edit that changes nothing is refused",
            &unanswered,
            false,
            edit(7, "And with kids?", vec![image(0)]),
        ),
        plan_case(
            "an edit of a reply's tool result edits the reply",
            &kyoto,
            false,
            edit(5, "One day: Kinkaku-ji.", vec![]),
        ),
        plan_case(
            "an edited reply carries no image",
            &kyoto,
            false,
            edit(2, "Day 1: gardens", vec![image(0)]),
        ),
        plan_case(
            "a regenerate branches after the question",
            &kyoto,
            false,
            Regenerate { message_id: 6 },
        ),
        plan_case(
            "a question is not regenerated",
            &kyoto,
            false,
            Regenerate { message_id: 3 },
        ),
        plan_case(
            "a greeting answers no question",
            &greeted,
            false,
            Regenerate { message_id: 1 },
        ),
        plan_case(
            "a branch copies to the end of the turn",
            &kyoto,
            false,
            Branch { message_id: 4 },
        ),
        plan_case(
            "a branch at a question copies to it",
            &unanswered,
            false,
            Branch { message_id: 7 },
        ),
        plan_case(
            "a message of another chat is not found",
            &kyoto,
            false,
            Branch { message_id: 99 },
        ),
    ]
}

fn points_cases() -> Vec<PointsCase> {
    let family = family();
    vec![
        points_case(
            "the first chat sees an edited question and a regenerated reply",
            10,
            &family,
        ),
        points_case(
            "the edited chat sees the question it edited, shown by the chat changed last",
            20,
            &family,
        ),
        points_case("the regenerated chat sees both its points", 30, &family),
        points_case("a branch with nothing yet is an empty option", 40, &family),
        points_case("a chat alone has no branch point", 10, &family[..1]),
    ]
}

fn recorded() -> Recorded {
    Recorded {
        about: "gglib's branching rules (crates/gglib-core/src/domain/branching), as cases. \
                Recorded by contract_tests.rs; change the rules there, never this file."
            .to_owned(),
        plans: plan_cases(),
        points: points_cases(),
    }
}

fn recorded_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../contracts/chats/branching.json")
}

/// The checked-in file is exactly what the rules answer. Run with
/// `GGLIB_RECORD_CONTRACTS=1` to rewrite it after a deliberate change.
#[test]
fn recorded_cases_match_the_checked_in_file() {
    let mut want = serde_json::to_string_pretty(&recorded()).expect("serialise");
    want.push('\n');
    let path = recorded_path();
    if std::env::var_os("GGLIB_RECORD_CONTRACTS").is_some() {
        std::fs::write(&path, &want).expect("write branching.json");
    }
    let have = std::fs::read_to_string(&path).expect("read contracts/chats/branching.json");
    assert!(
        have == want,
        "contracts/chats/branching.json is stale; rerun with GGLIB_RECORD_CONTRACTS=1\n{want}"
    );
}

/// Every case in the file, read back, is what the rules answer to its own
/// inputs: the file replays as ggchat replays it.
#[test]
fn every_recorded_case_replays_from_its_inputs() {
    let file = std::fs::read_to_string(recorded_path()).expect("read branching.json");
    let decoded: Recorded = serde_json::from_str(&file).expect("decode branching.json");
    for case in &decoded.plans {
        let path = messages(&case.path);
        assert_eq!(
            answer(&path, &case.change, case.busy),
            case.answer,
            "{}",
            case.name
        );
        assert_eq!(answerable(&path).is_ok(), case.answerable, "{}", case.name);
    }
    for case in &decoded.points {
        assert_eq!(points(case.me, &case.family), case.points, "{}", case.name);
    }
}
