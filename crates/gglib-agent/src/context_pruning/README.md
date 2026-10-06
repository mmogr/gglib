# Context Pruning

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

The loop holds its messages in a [`Pruned`], which prunes them and counts
what each prune dropped. The count runs over the whole run, and every model
call's `turn_usage` reports it as `trimmed_messages`: the messages missing
from the request that call answered. Nothing dropped is no key at all.

<!-- module-docs:end -->
