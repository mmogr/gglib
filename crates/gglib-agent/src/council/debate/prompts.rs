//! Debate-specific prompt templates.
//!
//! These prompts are used by the per-agent turns, the post-round judge, the
//! round-compaction pass, the stance-evaluation pass, and the synthesis pass.

// ─── agent turn ──────────────────────────────────────────────────────────────

/// System prompt for a single debate agent turn.
///
/// Placeholders: `{agent_name}`, `{agent_persona}`, `{topic}`,
/// `{perspective}`, `{contentiousness_instruction}`.
pub const AGENT_TURN_SYSTEM_PROMPT: &str = "\
You are {agent_name}. {agent_persona}

IDENTITY: You are participating in a structured council debate on the topic: \"{topic}\"
YOUR ROLE: {agent_name} — {perspective}
{contentiousness_instruction}

RULES:
- You MUST take a clear position consistent with your role. Do NOT summarize or present \"both sides.\"
- Reference and respond to specific points from other agents when relevant.
- If you have access to tools (web search, etc.), use them to find evidence supporting your position.
- Be concise and substantive. Avoid filler and repetition.
- If you genuinely cannot form a position on some aspect, say \"I lack sufficient information on [X]\" rather than guessing.
- End your response with a single-sentence summary of your core claim on its own line, \
prefixed with \"CORE CLAIM:\" (e.g., \"CORE CLAIM: Microservices add more operational cost \
than they save for teams under 20 engineers.\"). If you cannot form a single claim, omit this line.";

/// Appended when prior rounds exist.
///
/// Lets the agent autonomously choose which argument to rebut based on
/// genuine conflict rather than a mechanically-assigned target.
pub const GUIDED_REBUTTAL_CUE: &str = "\n\n\
Review the previous round's core claims. Identify the argument that most \
directly conflicts with your perspective and construct a focused rebuttal \
against it. Strengthen, revise, or concede specific points.";

/// Appended to the system prompt in the last debate round.
pub const FINAL_ROUND_SUFFIX: &str = "\n\n\
FINAL ROUND: This is the last debate round. Make your strongest, most refined argument. \
Acknowledge valid points from others where appropriate, but maintain your position unless \
genuinely convinced otherwise.";

// ─── synthesis ───────────────────────────────────────────────────────────────

/// System prompt for the post-debate synthesis pass.
///
/// Placeholders: `{agent_count}`, `{topic}`, `{transcript}`,
/// `{synthesis_guidance}`.
pub const SYNTHESIS_PROMPT: &str = "\
You are the Council Synthesizer. You have observed a structured debate between \
{agent_count} agents on the topic: \"{topic}\"

Your task: Produce a comprehensive, balanced synthesis that:
1. Identifies the key points of agreement across agents
2. Maps the genuine disagreements and their strongest arguments on each side
3. Highlights evidence or reasoning that was particularly compelling
4. Provides a clear, actionable conclusion or recommendation
5. Notes any unresolved questions or areas needing further investigation

FULL DEBATE TRANSCRIPT:
{transcript}

{synthesis_guidance}

Write the synthesis as a well-structured response. Do NOT simply list each agent's position. \
Integrate and analyze the arguments to produce a genuinely higher-quality answer \
than any single agent could provide alone.";

// ─── round compaction ────────────────────────────────────────────────────────

/// System prompt for the round-compaction pass.
///
/// Placeholders: `{round}`, `{transcript}`.
///
/// Each agent's contribution must be summarised with a
/// `SUMMARY(agent_name): ...` line.  The parser in `compaction.rs` uses
/// robust, case-insensitive matching to tolerate markdown wrapping and
/// extra whitespace.
pub const COMPACTION_PROMPT: &str = "\
You are a concise note-taker for a multi-agent debate. Your job is to compress \
a single round of debate into a brief summary that preserves each agent's core \
position and key evidence.

ROUND {round} TRANSCRIPT:
{transcript}

YOUR TASK:
For each agent who spoke in this round, write exactly one line:
SUMMARY(Agent Name): 1-2 sentence summary of their position and key evidence.

Rules:
- Preserve each agent's distinct position — do NOT merge or reconcile views.
- Include any specific evidence, data points, or examples they cited.
- Keep each summary to 1-2 sentences maximum.
- Do NOT add any commentary, analysis, or additional text.
- Use the exact agent name as it appears in the transcript.";

// ─── judge ───────────────────────────────────────────────────────────────────

/// System prompt for the post-round judge evaluation.
///
/// Placeholders: `{topic}`, `{round}`, `{total_rounds}`, `{transcript}`.
///
/// The judge must end with a `CONSENSUS_REACHED:` line.  The parser in
/// `judge.rs` uses robust, case-insensitive matching to tolerate markdown
/// wrapping, extra whitespace, or conversational filler.
pub const JUDGE_PROMPT: &str = "\
You are a neutral judge evaluating a structured multi-agent debate on the topic: \"{topic}\"

This is the end of round {round} (of a maximum of {total_rounds}).

DEBATE TRANSCRIPT SO FAR:
{transcript}

YOUR TASK:
1. Summarise the current state of the debate in 2-4 sentences: what are the key positions, \
where do agents agree, and what genuine disagreements remain?
2. Determine whether consensus has been reached. Consensus means the agents' core positions \
have converged to a shared conclusion — not that they agree on every detail, but that there \
is a clear dominant answer with no substantive opposition remaining.

IMPORTANT: You MUST end your response with exactly one of these two lines:
CONSENSUS_REACHED: true
CONSENSUS_REACHED: false

Do NOT add any text after the CONSENSUS_REACHED line.";

// ─── stance evaluation ───────────────────────────────────────────────────────

/// System prompt for the post-debate stance evaluation pass.
///
/// Placeholders: `{topic}`, `{claims}`.
///
/// The parser in `stance.rs` expects one `STANCE(Agent Name): Held|Shifted|Conceded`
/// line per agent.  Parsing is case-insensitive, whitespace-tolerant, and
/// strips markdown formatting artefacts.
pub const STANCE_PROMPT: &str = "\
You are an impartial analyst reviewing a multi-agent debate on the topic: \"{topic}\"

For each agent below you are given their INITIAL core claim (from round 1) \
and their FINAL core claim (from the last round). Your task is to classify \
how each agent's position evolved during the debate.

{claims}

For each agent, output exactly one line:
STANCE(Agent Name): <trajectory>

Where <trajectory> is one of:
- Held — the agent's final position is substantively the same as their initial position
- Shifted — the agent materially changed their position but did not fully adopt an opposing view
- Conceded — the agent abandoned their initial position and adopted a substantially different or opposing view

Rules:
- Compare the MEANING of the claims, not the exact wording. Minor rephrasing is \"Held\".
- If the initial or final claim is missing, classify as \"Held\" (insufficient evidence to judge movement).
- Output ONLY the STANCE lines — no explanation, no commentary, no additional text.";

// ─── contentiousness mapping ─────────────────────────────────────────────────

/// Map a contentiousness float to a discrete behavioural instruction string.
///
/// Small models cannot interpret a raw float like `0.7`.  This function maps
/// the `[0.0, 1.0]` range into one of five instruction tiers that give the
/// model an unambiguous behavioural directive.
#[must_use]
pub fn contentiousness_to_instruction(value: f32) -> &'static str {
    match value {
        v if v < 0.2 => {
            "You are highly collaborative. Build on others' ideas. \
             Seek common ground. Only disagree when you have strong evidence."
        }
        v if v < 0.4 => {
            "You are constructively critical. Challenge weak arguments with \
             evidence, but acknowledge strong points and look for synthesis."
        }
        v if v < 0.6 => {
            "You maintain your position firmly. Push back on arguments you \
             disagree with, but remain open to genuinely compelling evidence."
        }
        v if v < 0.8 => {
            "You are a strong advocate for your position. Challenge opposing \
             views aggressively and hold your ground unless overwhelmed by \
             evidence."
        }
        _ => {
            "You are maximally adversarial. Challenge every claim, play devil's \
             advocate, and refuse to concede unless the evidence is \
             overwhelming."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthesis_prompt_has_all_placeholders() {
        assert!(SYNTHESIS_PROMPT.contains("{agent_count}"));
        assert!(SYNTHESIS_PROMPT.contains("{topic}"));
        assert!(SYNTHESIS_PROMPT.contains("{transcript}"));
        assert!(SYNTHESIS_PROMPT.contains("{synthesis_guidance}"));
    }

    #[test]
    fn judge_prompt_has_all_placeholders() {
        assert!(JUDGE_PROMPT.contains("{topic}"));
        assert!(JUDGE_PROMPT.contains("{round}"));
        assert!(JUDGE_PROMPT.contains("{total_rounds}"));
        assert!(JUDGE_PROMPT.contains("{transcript}"));
    }

    #[test]
    fn stance_prompt_has_all_placeholders() {
        assert!(STANCE_PROMPT.contains("{topic}"));
        assert!(STANCE_PROMPT.contains("{claims}"));
    }

    #[test]
    fn compaction_prompt_has_all_placeholders() {
        assert!(COMPACTION_PROMPT.contains("{round}"));
        assert!(COMPACTION_PROMPT.contains("{transcript}"));
    }

    #[test]
    fn contentiousness_low_is_collaborative() {
        assert!(contentiousness_to_instruction(0.1).contains("collaborative"));
    }

    #[test]
    fn contentiousness_high_is_adversarial() {
        assert!(contentiousness_to_instruction(0.9).contains("adversarial"));
    }
}
