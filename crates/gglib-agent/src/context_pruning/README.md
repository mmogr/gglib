# Context Pruning

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-context_pruning-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-context_pruning-complexity.json)

<!-- module-docs:start -->

Context-budget pruning for the agentic loop.

Long agentic runs accumulate tool messages that can exceed the LLM's context
window.  This module trims the conversation history when the total character
count exceeds [`AgentConfig::context_budget_chars`], applying two passes:

1. **Tool-message pruning** ([`tool_pruning`]) — keep only the most recent
   [`AgentConfig::prune_keep_tool_messages`] tool results and drop the
   corresponding `Assistant` messages whose every tool call was removed.
2. **Tail pruning** ([`tail_pruning`]) — if still over budget after pass 1,
   keep all `System` messages and the trailing
   [`AgentConfig::prune_keep_tail_messages`] non-system messages.

<!-- module-docs:end -->
