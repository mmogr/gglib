# normalize

<!-- module-docs:start -->

What gglib reads in a model's text, around the request and the reply.
gglib does not translate tool-call dialects: llama-server parses the calls
it can, and anything it leaves in `content` reaches the client as it was
written.

## Module map

- [`residue`] — chunk-safe scanner for tool-call markup that reached
  client-visible text (the proxy's drift alarm), and [`no_answer`], which
  says when a run's final reply is not an answer: no words, or a tool call
  written as text on a turn that offered no tools.
- [`history`] — [`strip_thinking_debt`], which drops reasoning left in
  earlier assistant turns of a chat's history; stage 1 of the request
  pipeline calls it.

<!-- module-docs:end -->
