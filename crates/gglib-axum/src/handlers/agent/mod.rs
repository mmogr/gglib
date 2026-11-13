#![doc = include_str!("README.md")]
mod chat_sampling;
mod chat_turn;
mod compose;
mod device_tools;
mod hub_model;
pub(crate) mod hub_turn;
mod image_gate;
mod launch;
mod loop_request;
mod remote_upstream;
mod retry_notice;
mod run;
mod slot_line;
mod transcript;

pub(crate) use slot_line::{AGENT_SLOT_LINE, AGENT_SLOT_WAIT, SlotLine};
pub(crate) use transcript::{keep_a_model, keep_machine};

#[cfg(test)]
mod endpoint_fixture;
#[cfg(test)]
mod image_model_tests;
#[cfg(test)]
#[path = "run_answer_tests.rs"]
mod run_answer_tests;
#[cfg(test)]
#[path = "run_busy_tests.rs"]
mod run_busy_tests;
#[cfg(test)]
#[path = "run_end_tests.rs"]
mod run_end_tests;
#[cfg(test)]
pub(in crate::handlers) mod run_fixture;
#[cfg(test)]
#[path = "run_folder_tests.rs"]
mod run_folder_tests;
#[cfg(test)]
#[path = "run_machine_tests.rs"]
mod run_machine_tests;
#[cfg(test)]
#[path = "run_made_tests.rs"]
mod run_made_tests;
#[cfg(test)]
#[path = "run_model_tests.rs"]
mod run_model_tests;
#[cfg(test)]
#[path = "run_preview_tests.rs"]
mod run_preview_tests;
#[cfg(test)]
#[path = "run_privacy_tests.rs"]
mod run_privacy_tests;
#[cfg(test)]
#[path = "run_refusal_tests.rs"]
mod run_refusal_tests;
#[cfg(test)]
#[path = "run_rows_tests.rs"]
mod run_rows_tests;
#[cfg(test)]
#[path = "run_tests.rs"]
mod run_tests;
#[cfg(test)]
#[path = "run_thinking_tests.rs"]
mod run_thinking_tests;
#[cfg(test)]
#[path = "transcript_images_tests.rs"]
mod transcript_images_tests;
#[cfg(test)]
mod turn_fixture;
