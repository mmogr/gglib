//! Prompt templates and contentiousness mapping for council deliberations.
//!
//! All prompts are plain string constants with `{placeholder}` markers that
//! callers fill via [`format!`].  The contentiousness float is mapped to a
//! discrete behavioural instruction via [`contentiousness_to_instruction`] so
//! that small models receive an unambiguous directive rather than a raw number.
//!
//! # Research references
//!
//! - `SocraSynth` (Stanford) — contentiousness parameter, Socratic method
//! - MAD / Multi-Agent Debate — structured debate topologies
//! - UT Austin — explicit stance-forcing to prevent summarisation collapse
//! - Debate-to-Write — persona → debate → synthesis pipeline

// ─── council designer ────────────────────────────────────────────────────────

/// System prompt for the `/api/council/suggest` endpoint.
///
/// Placeholders: `{agent_count}`, `{user_topic}`.
pub const COUNCIL_DESIGNER_PROMPT: &str = "\
You are a council architect. Given a user's question or topic, design a council \
of approximately {agent_count} agents who will deliberate on it from diverse perspectives.

For each agent, provide:
- id: A short kebab-case identifier (e.g., \"devils-advocate\", \"domain-expert\")
- name: A concise role title (e.g., \"Devil's Advocate\", \"Domain Expert\", \"Pragmatist\")
- persona: 2-3 sentences defining their worldview, expertise, and argumentative style
- contentiousness: A number 0.0-1.0 (0.0 = fully collaborative, 1.0 = maximally adversarial)
- perspective: One sentence describing what unique angle they bring

Rules:
- Agents MUST have genuinely different perspectives, not just different phrasings of agreement
- At least one agent should be skeptical/adversarial (contentiousness >= 0.7)
- At least one agent should be constructive/solution-oriented (contentiousness <= 0.4)
- Personas should be specific enough that a small language model can consistently role-play them

Also suggest:
- rounds: How many debate rounds (typically 2-4; more for complex/controversial topics)
- synthesis_guidance: A brief note on what the final synthesis should prioritize

Respond with ONLY the JSON object below — no explanation, no markdown fences, \
no surrounding text:
{{ \"agents\": [...], \"rounds\": N, \"synthesis_guidance\": \"...\" }}

Topic: \"{user_topic}\"";

/// Addendum appended to the designer system prompt during refinement.
///
/// Instructs the LLM to make minimal, targeted changes and preserve
/// stable agent IDs so the frontend can diff old vs new suggestions.
pub const COUNCIL_REFINEMENT_ADDENDUM: &str = "\n\n\
IMPORTANT — you are REFINING a previous suggestion based on user feedback.\n\
- Make MINIMAL changes. Preserve agents the user does not mention.\n\
- Keep the `id` field IDENTICAL for agents you do not modify.\n\
- Only add, remove, or modify agents the user specifically requests.\n\
- You may adjust the agent count freely — ignore the original count guidance.\n\
- Respond with the COMPLETE updated JSON (all agents, not just changes).";

// ─── agent turn ──────────────────────────────────────────────────────────────

/// System prompt injected at the start of every agent turn.
///
/// Placeholders: `{agent_name}`, `{agent_persona}`, `{topic}`,
/// `{perspective}`, `{contentiousness}`, `{contentiousness_instruction}`.
///
/// The debate history and final-round blocks are appended separately by
/// [`crate::council::history::build_agent_system_prompt`].
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

/// Appended to the system prompt when the agent has prior rounds to respond to
/// but no directed rebuttal target is available (no prior core claims).
pub const DEBATE_HISTORY_SUFFIX: &str = "\n\n\
Respond to the strongest counterarguments from previous rounds. \
Strengthen, revise, or concede specific points.";

/// Appended instead of [`DEBATE_HISTORY_SUFFIX`] when a directed rebuttal
/// target has been selected.
///
/// Placeholders: `{target_name}`, `{target_claim}`.
pub const TARGETED_REBUTTAL_CUE: &str = "\n\n\
DIRECTED REBUTTAL: You must directly address {target_name}'s core claim: \
\"{target_claim}\"\n\
Explain specifically why you agree or disagree with this position from your \
perspective. Strengthen, revise, or concede specific points — but do not \
ignore their argument.";

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
Integrate and analyze the arguments to produce a genuinely higher-quality answer than any \
single agent could provide alone.";

// ─── contentiousness mapping ─────────────────────────────────────────────────

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
            "You are constructive but independent. Offer your own perspective. \
             Agree when warranted, but push back on weak reasoning."
        }
        v if v < 0.6 => {
            "You are balanced. Evaluate each argument on its merits. \
             Challenge unsupported claims. Credit strong points from others."
        }
        v if v < 0.8 => {
            "You are a rigorous critic. Actively look for flaws, assumptions, \
             and gaps in others' arguments. Demand evidence for claims."
        }
        _ => {
            "You are a devil's advocate. Systematically challenge every argument. \
             Assume the opposing position. Force others to defend their reasoning \
             under pressure."
        }
    }
}

/// Return the human-readable tier label for a contentiousness value.
///
/// Useful for UI display alongside the slider.
#[must_use]
pub fn contentiousness_tier_label(value: f32) -> &'static str {
    match value {
        v if v < 0.2 => "Collaborative",
        v if v < 0.4 => "Constructive",
        v if v < 0.6 => "Balanced",
        v if v < 0.8 => "Adversarial",
        _ => "Devil's Advocate",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contentiousness_boundaries() {
        assert!(contentiousness_to_instruction(0.0).contains("collaborative"));
        assert!(contentiousness_to_instruction(0.19).contains("collaborative"));
        assert!(contentiousness_to_instruction(0.2).contains("constructive"));
        assert!(contentiousness_to_instruction(0.4).contains("balanced"));
        assert!(contentiousness_to_instruction(0.6).contains("rigorous critic"));
        assert!(contentiousness_to_instruction(0.8).contains("devil's advocate"));
        assert!(contentiousness_to_instruction(1.0).contains("devil's advocate"));
    }

    #[test]
    fn tier_labels() {
        assert_eq!(contentiousness_tier_label(0.0), "Collaborative");
        assert_eq!(contentiousness_tier_label(0.3), "Constructive");
        assert_eq!(contentiousness_tier_label(0.5), "Balanced");
        assert_eq!(contentiousness_tier_label(0.7), "Adversarial");
        assert_eq!(contentiousness_tier_label(0.9), "Devil's Advocate");
    }

    #[test]
    fn designer_prompt_has_placeholders() {
        assert!(COUNCIL_DESIGNER_PROMPT.contains("{agent_count}"));
        assert!(COUNCIL_DESIGNER_PROMPT.contains("{user_topic}"));
    }

    #[test]
    fn agent_turn_prompt_has_placeholders() {
        assert!(AGENT_TURN_SYSTEM_PROMPT.contains("{agent_name}"));
        assert!(AGENT_TURN_SYSTEM_PROMPT.contains("{agent_persona}"));
        assert!(AGENT_TURN_SYSTEM_PROMPT.contains("{topic}"));
        assert!(AGENT_TURN_SYSTEM_PROMPT.contains("{contentiousness_instruction}"));
    }

    #[test]
    fn synthesis_prompt_has_placeholders() {
        assert!(SYNTHESIS_PROMPT.contains("{agent_count}"));
        assert!(SYNTHESIS_PROMPT.contains("{topic}"));
        assert!(SYNTHESIS_PROMPT.contains("{transcript}"));
        assert!(SYNTHESIS_PROMPT.contains("{synthesis_guidance}"));
    }

    #[test]
    fn negative_contentiousness_treated_as_collaborative() {
        assert!(contentiousness_to_instruction(-0.5).contains("collaborative"));
    }

    #[test]
    fn judge_prompt_has_placeholders() {
        assert!(JUDGE_PROMPT.contains("{topic}"));
        assert!(JUDGE_PROMPT.contains("{round}"));
        assert!(JUDGE_PROMPT.contains("{total_rounds}"));
        assert!(JUDGE_PROMPT.contains("{transcript}"));
    }

    #[test]
    fn rebuttal_cue_has_placeholders() {
        assert!(TARGETED_REBUTTAL_CUE.contains("{target_name}"));
        assert!(TARGETED_REBUTTAL_CUE.contains("{target_claim}"));
    }
}
