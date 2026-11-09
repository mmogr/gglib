//! The state words, written to `contracts/vocabulary/words.json`.
//!
//! `gglib_core::domain::state_words` is the one place a state's word lives,
//! and `ChoiceSource::word` the one place a launch choice's source has its
//! words. The test here writes every state and every source with its wire
//! name and its word to the contract file, and fails when the file goes
//! stale, so the GUI's word tables (held to that file by
//! `tests/ts/contracts/stateWords.test.ts`) cannot drift from what the CLI
//! prints.

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use gglib_core::domain::{
        ChoiceSource, EndpointState, EngineState, FastDownloadsState, KeyState, McpState,
        ModelsDirState, ReachState, ResidencyState,
    };
    use serde::Serialize;

    /// One state: its name on the wire and the word a person reads.
    #[derive(Serialize)]
    struct Recorded {
        state: String,
        word: &'static str,
    }

    /// The tables as the file holds them, each in display order: the state
    /// tables, and whose choice a launch argument is.
    #[derive(Serialize)]
    struct Words {
        endpoint: Vec<Recorded>,
        residency: Vec<Recorded>,
        reach: Vec<Recorded>,
        key: Vec<Recorded>,
        choice: Vec<Recorded>,
        engine: Vec<Recorded>,
        tool_server: Vec<Recorded>,
        fast_downloads: Vec<Recorded>,
        models_dir: Vec<Recorded>,
    }

    fn row<S: Serialize>(state: S, word: &'static str) -> Recorded {
        let state = serde_json::to_value(&state).expect("serialise");
        let state = state
            .as_str()
            .expect("a state is a string on the wire")
            .to_owned();
        Recorded { state, word }
    }

    fn recorded() -> Words {
        Words {
            endpoint: EndpointState::ALL
                .iter()
                .map(|s| row(s, s.word()))
                .collect(),
            residency: ResidencyState::ALL
                .iter()
                .map(|s| row(s, s.word()))
                .collect(),
            reach: ReachState::ALL.iter().map(|s| row(s, s.word())).collect(),
            key: KeyState::ALL.iter().map(|s| row(s, s.word())).collect(),
            choice: ChoiceSource::ALL.iter().map(|s| row(s, s.word())).collect(),
            engine: EngineState::ALL.iter().map(|s| row(s, s.word())).collect(),
            tool_server: McpState::ALL.iter().map(|s| row(s, s.word())).collect(),
            fast_downloads: FastDownloadsState::ALL
                .iter()
                .map(|s| row(s, s.word()))
                .collect(),
            models_dir: ModelsDirState::ALL
                .iter()
                .map(|s| row(s, s.word()))
                .collect(),
        }
    }

    fn recorded_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../contracts/vocabulary/words.json")
    }

    /// The checked-in file is exactly what the tables give. Run with
    /// `GGLIB_RECORD_CONTRACTS=1` to rewrite it after a deliberate change.
    #[test]
    fn the_checked_in_words_are_the_tables() {
        let mut want = serde_json::to_string_pretty(&recorded()).expect("serialise");
        want.push('\n');
        let path = recorded_path();
        if std::env::var_os("GGLIB_RECORD_CONTRACTS").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).expect("make contracts/vocabulary");
            std::fs::write(&path, &want).expect("write words.json");
        }
        let have = std::fs::read_to_string(&path).expect("read contracts/vocabulary/words.json");
        assert!(
            have == want,
            "contracts/vocabulary/words.json is stale; rerun with GGLIB_RECORD_CONTRACTS=1\n{want}"
        );
    }
}
