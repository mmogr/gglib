# peg-native: a tool-argument value containing `</parameter>` silently absorbs the following parameter's markup

## Summary

For models using the constructed (XML-style) tool-call dialect, an argument
value that contains a literal `</parameter>` does not terminate the value.
The scan continues and swallows the markup of the *next* parameter, producing
a tool call that is schema-valid but carries a corrupted value.

Nothing surfaces. No error, no warning, no parse failure — the caller receives
a well-formed `tool_calls` entry whose argument silently contains dialect
markup that was never part of the value.

This appears to be the successor to #24807 (closed by #24839): that issue
reported the parser *dropping* the call and aborting the stream. On current
master the call survives, but with a corrupted argument, which is quieter and
correspondingly harder to notice.

Reproduced 5/5 at temperature 0.7, and again 2/2 at a later run.

## Environment

| | |
|---|---|
| Commit | `69bf643` ("CUDA: fix thread/block count in quantized cpy kernel launches") |
| Built with | Clang 22.1.8, Linux x86_64 |
| Model | `unsloth/Qwen3.5-4B-MTP-GGUF` — `Qwen3.5-4B-UD-Q6_K_XL.gguf` |
| Server | `llama-server -m <model> --jinja --port 8081 -c 8192 -ngl 99` |

## Reproducer

```bash
curl -s http://127.0.0.1:8081/v1/chat/completions \
  -H 'Content-Type: application/json' -d '{
  "messages": [{"role": "user", "content":
    "Read the file whose name is exactly a</parameter>b in text mode. The filename really does contain that text — pass it through unchanged."}],
  "tools": [{"type": "function", "function": {
    "name": "read_file",
    "parameters": {
      "type": "object",
      "properties": {
        "path": {"type": "string"},
        "mode": {"type": "string", "enum": ["text", "binary"]}
      },
      "required": ["path", "mode"],
      "additionalProperties": false
    }}}],
  "tool_choice": "auto",
  "temperature": 0.7,
  "max_tokens": 1024
}'
```

**Expected** — the filename passed through unchanged:

```json
{"path": "a</parameter>b", "mode": "text"}
```

**Actual** — `path` has absorbed the `mode` parameter's opening markup:

```json
{"path": "a</parameter>\n<parameter=mode>\ntext", "mode": "text"}
```

Note that `mode` still arrives correctly, and every schema constraint is
satisfied: both required keys present, both strings, `mode` within its enum,
no additional properties. A consumer validating against the tool's JSON Schema
sees a perfectly conformant call. Only the *content* of `path` is wrong.

## The model intended the right thing

From the same response's `reasoning_content`, so this is not the model being
confused about the task:

```
The user wants me to read a file with the exact name "a</parameter>b" in text
mode. They've clarified that the filename really does contain that text and I
should pass it through unchanged.

Let me call the read_file function with:
- path: "a</parameter>b"
- mode: "text"
```

The model knew exactly what it wanted to emit. The dialect gave it no way to
say it.

## Why it happens

The generated grammar (from `--verbose`) opens each parameter with a trailing
newline and terminates the value on a **newline-anchored** close marker:

```
tool-read-file-arg-path ::= ("<parameter=" "path" ">\n") xml-arg-string
tool-read-file-arg-mode ::= ("<parameter=" "mode" ">\n") xml-arg-string

xml-arg-string ::= ac-20
ac-20    ::= [\n] ac-20-01 | [^\n] ac-20
ac-20-01 ::= [\n] ac-20-01 | [<] ac-20-02 | [^\n<] ac-20
```

`ac-20` only leaves the value state on `\n` followed by `</parameter>`. A bare
`</parameter>` — not preceded by a newline — is an ordinary run of characters
and the value continues.

The model, reasonably, believes `</parameter>` closes a parameter. So it emits
one. The grammar does not agree, the value keeps accumulating across the
following `\n<parameter=mode>\ntext`, and only the *next* `\n</parameter>`
closes `path`.

Model's mental model and the grammar's close condition disagree, and the
disagreement is resolved silently in favour of the grammar.

## This is an expressiveness gap, not a boundary bug

Worth stating plainly, because it constrains what a fix can be: the dialect has
**no escaping mechanism**. Given the byte sequence

```
a</parameter>
<parameter=mode>
text
</parameter>
```

there is no principled way to recover whether the author meant
`path="a"` + `mode="text"`, or `path="a</parameter>\n<parameter=mode>\ntext"`.
Both readings are consistent with the text. Any parser — this one or another —
is choosing a heuristic, not deriving an answer.

So this is not a one-line boundary fix, and I am deliberately not proposing a
patch that hard-codes one reading over the other.

## Possible directions

Listed as trade-offs rather than a recommendation, since the choice belongs to
maintainers:

1. **Accept a bare `</parameter>` as a close.** `path` would come back as
   `"a"` — truncated rather than corrupted. A clean, visible failure beats
   silent corruption, but it makes values containing the marker
   unrepresentable at all.
2. **Add escaping to the dialect** (entity-style, or a length prefix). The only
   option that makes the value space complete, and the most invasive — it needs
   template-side agreement.
3. **Detect and reject.** Keep current parsing, but if a captured value
   contains `<parameter=` or the function opener, treat the call as malformed
   and surface an error. Does not recover the intent, but converts a silent
   wrong answer into a loud one.
4. **Bound the value scan at the next `<parameter=` opener** when the schema
   declares no parameter whose value could plausibly contain markup. Still a
   heuristic, and schema-dependent.

Option 3 alone would be a meaningful improvement even without the others,
because the current failure mode is indistinguishable from success at the API
boundary.

## Why this matters beyond the pathological case

The reproducer uses a deliberately hostile filename, which is rare. But the
same mechanism fires on any value that legitimately contains the close marker
— quoted XML, HTML snippets, code containing the literal string, or a diff
being passed to a patch tool. Agentic workloads pass file contents and code
fragments as tool arguments routinely, and in those cases the corruption lands
in an argument that a downstream tool then acts on.
