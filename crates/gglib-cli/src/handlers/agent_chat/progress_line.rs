//! The words of a long tool's progress, and of a wait before a reply, for
//! the status line ([`crate::presentation::status_line`]): "drawing:
//! sampling 2/4", or "Loading Qwen3-4B… 12 s" while the daemon loads the
//! chat's model. The agent loop sends at most about one a second.

use gglib_core::domain::agent::{AgentEvent, ToolStage};

/// The words for a `ToolProgress` or `Waiting` event; `None` for any other.
pub(super) fn progress_line(event: &AgentEvent) -> Option<String> {
    match event {
        AgentEvent::ToolProgress {
            stage,
            pass,
            done,
            total,
            position,
            ..
        } => Some(match stage {
            ToolStage::Queued => position.map_or_else(
                || "drawing: queued".to_owned(),
                |n| format!("drawing: queued, place {n}"),
            ),
            ToolStage::Loading => "drawing: loading…".to_owned(),
            ToolStage::Sampling => {
                let steps = match (done, total) {
                    (Some(d), Some(t)) => format!("sampling {d}/{t}"),
                    _ => "sampling".to_owned(),
                };
                match pass {
                    Some(p) if *p > 1 => format!("drawing: {steps}, image {p}"),
                    _ => format!("drawing: {steps}"),
                }
            }
            ToolStage::Decoding => "drawing: decoding".to_owned(),
            ToolStage::Finishing => "drawing: finishing".to_owned(),
        }),
        // A wait's words are core's, shared with every surface that shows
        // one: "Loading Qwen3-4B… 12 s".
        AgentEvent::Waiting { .. } => event.waiting_sentence(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progress(stage: ToolStage, pass: Option<u32>, done: Option<u32>) -> AgentEvent {
        AgentEvent::ToolProgress {
            tool_call_id: "c1".into(),
            stage,
            pass,
            done,
            total: done.map(|_| 4),
            position: None,
        }
    }

    #[test]
    fn each_stage_reads_as_the_brief_says() {
        let lines: Vec<String> = [
            progress(ToolStage::Loading, None, None),
            progress(ToolStage::Sampling, Some(1), Some(2)),
            progress(ToolStage::Sampling, Some(2), Some(1)),
            progress(ToolStage::Decoding, None, None),
            progress(ToolStage::Finishing, None, None),
        ]
        .iter()
        .filter_map(progress_line)
        .collect();
        assert_eq!(
            lines,
            [
                "drawing: loading…",
                "drawing: sampling 2/4",
                "drawing: sampling 1/4, image 2",
                "drawing: decoding",
                "drawing: finishing",
            ]
        );
    }

    #[test]
    fn a_queued_tool_says_its_place() {
        let queued = AgentEvent::ToolProgress {
            tool_call_id: "c1".into(),
            stage: ToolStage::Queued,
            pass: None,
            done: None,
            total: None,
            position: Some(2),
        };
        assert_eq!(
            progress_line(&queued).as_deref(),
            Some("drawing: queued, place 2")
        );
    }

    #[test]
    fn a_wait_is_said_in_cores_words_with_the_model_and_its_seconds() {
        use gglib_core::domain::agent::WaitingFor;
        let wait = AgentEvent::Waiting {
            reason: WaitingFor::ModelLoad,
            step: 0,
            total: 0,
            position: 0,
            model: Some("Qwen3-4B".to_owned()),
            elapsed_ms: Some(12_000),
            said: None,
        };
        assert_eq!(
            progress_line(&wait).as_deref(),
            Some("Loading Qwen3-4B… 12 s")
        );
    }

    #[test]
    fn a_wait_for_an_agent_slot_is_said_in_cores_words() {
        let wait = AgentEvent::agent_slot_wait(2, 5_000);
        assert_eq!(
            progress_line(&wait).as_deref(),
            Some("Waiting for a free agent slot, 2 in line… 5 s")
        );
    }

    #[test]
    fn a_preview_and_every_other_event_have_no_line() {
        let preview = AgentEvent::ToolPreview {
            tool_call_id: "c1".into(),
            frame: gglib_core::domain::agent::PreviewFrame::png(1, 4, "iVBO"),
        };
        assert_eq!(progress_line(&preview), None);
        assert_eq!(
            progress_line(&AgentEvent::FinalAnswer {
                content: "x".into()
            }),
            None
        );
    }
}
