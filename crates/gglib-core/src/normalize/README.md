# normalize

<!-- module-docs:start -->

What gglib reads in a model's text, around the request and the reply.
gglib does not translate tool-call dialects: llama-server parses the calls
it can, and anything it leaves in `content` reaches the client as it was
written.

## Module map

- [`tags`] — `format:*` constants, the tags detection stores on a model.
- [`residue`] — chunk-safe scanner for tool-call markup that reached
  client-visible text (the proxy's drift alarm).
- [`history`] — [`strip_thinking_debt`], which drops reasoning left in
  earlier assistant turns of a chat's history; stage 1 of the request
  pipeline calls it.

<!-- module-docs:end -->
