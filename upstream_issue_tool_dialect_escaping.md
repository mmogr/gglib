# peg-native: tool-argument values containing dialect delimiters cause a 500 (JSON dialect) or silent corruption (XML dialect)

## Summary

Tool-call dialects have no way to escape their own delimiters inside an
argument value. The consequence differs by dialect family, and one of the two
is a hard failure:

- **JSON dialect (Llama 3.2 3B): HTTP 500.** A value containing `{` and `"`
  makes the peg-native parser fail and the whole request errors with
  `"The model produced output that does not match the expected peg-native format"`.
  Not a bad tool call — a dead request.
- **XML dialect (Qwen3.5 4B): silent corruption.** A value containing
  `</parameter>` absorbs the following parameter's markup. The caller receives
  a schema-valid `tool_calls` entry whose argument silently contains dialect
  markup that was never part of the value. No error, no warning.

Both reproduce on demand on current master. The 500 is filed first because a
parse failure arguably should degrade to returning the text as content rather
than erroring the request, which looks like a contained fix independent of the
larger escaping question.

This appears to be the successor to #24807 (closed by #24839): that issue
reported the parser dropping a call and aborting the stream. The failure has
not gone away, it has split into these two shapes.

## Environment

| | |
|---|---|
| Commit | `69bf643` ("CUDA: fix thread/block count in quantized cpy kernel launches", released as `b10327`) |
| Built with | Clang 22.1.8, Linux x86_64 |
| Models | `unsloth/Llama-3.2-3B-Instruct-GGUF` — `Llama-3.2-3B-Instruct-UD-Q6_K_XL.gguf`<br>`unsloth/Qwen3.5-4B-MTP-GGUF` — `Qwen3.5-4B-UD-Q6_K_XL.gguf` |
| Server | `llama-server -m <model> --jinja --port 8081 -c 8192 -ngl 99` |

---

# Part 1 — HTTP 500 on the JSON dialect (Llama 3.2 3B)

## Reproducer

```bash
curl -s -w '\nHTTP:%{http_code}\n' http://127.0.0.1:8081/v1/chat/completions \
  -H 'Content-Type: application/json' -d '{
  "messages": [{"role": "user", "content":
    "Read the file whose name is exactly {\"a\": \"b\"} in text mode. The filename really does contain those braces and quotes — pass it through unchanged."}],
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

**Actual:**

```json
{"error":{"code":500,"message":"The model produced output that does not match the expected peg-native format","type":"server_error"}}
HTTP:500
```

Observed in 3 of 5 sampled attempts, and reproduced 1/1 on demand afterwards.
The 2 attempts that did not 500 returned a wrong-typed `path` (once the single
character `{`, once a JSON object rather than a string).

**Expected:** either the filename passed through unchanged, or — if the parse
genuinely cannot be completed — the generated text returned as assistant
`content` with no `tool_calls`, which is what a client can actually handle.

## Why a 500 is the wrong failure mode here

The model produced output the server could not parse. That is a legitimate
thing to happen with a 3B model and an awkward value. But a 5xx tells the
caller *the server* failed, so OpenAI-compatible clients treat it as
retryable-or-fatal infrastructure trouble rather than a model-output problem.
Agentic harnesses generally either abort the session or retry the identical
request, which reproduces the identical failure.

Returning the raw text as `content` would let the caller see what happened and
recover in the normal way. A 4xx would at least attribute the failure
correctly.

---

# Part 2 — Silent corruption on the XML dialect (Qwen3.5 4B)

## Reproducer

Same server flags, Qwen3.5-4B, same tool schema:

```json
{"messages": [{"role": "user", "content":
  "Read the file whose name is exactly a</parameter>b in text mode. The filename really does contain that text — pass it through unchanged."}],
 "tools": [ ... same read_file ... ],
 "tool_choice": "auto", "temperature": 0.7, "max_tokens": 1024}
```

**Expected:**

```json
{"path": "a</parameter>b", "mode": "text"}
```

**Actual** (5/5 attempts) — `path` has absorbed the `mode` parameter's markup:

```json
{"path": "a</parameter>\n<parameter=mode>\ntext", "mode": "text"}
```

Every schema constraint is satisfied: both required keys present, both strings,
`mode` within its enum, no additional properties. A consumer validating against
the tool's JSON Schema sees a perfectly conformant call. Only the *content* of
`path` is wrong. This is the part that makes it dangerous — there is no signal
at the API boundary that anything went wrong.

## The model intended the right thing

From the same response's `reasoning_content`:

```
The user wants me to read a file with the exact name "a</parameter>b" in text
mode. They've clarified that the filename really does contain that text and I
should pass it through unchanged.

Let me call the read_file function with:
- path: "a</parameter>b"
- mode: "text"
```

The model knew what it wanted to emit. The dialect gave it no way to say it.

## Mechanism

The generated grammar (from `--verbose`) opens each parameter with a trailing
newline and terminates the value on a **newline-anchored** close:

```
tool-read-file-arg-path ::= ("<parameter=" "path" ">\n") xml-arg-string
tool-read-file-arg-mode ::= ("<parameter=" "mode" ">\n") xml-arg-string

xml-arg-string ::= ac-20
ac-20    ::= [\n] ac-20-01 | [^\n] ac-20
ac-20-01 ::= [\n] ac-20-01 | [<] ac-20-02 | [^\n<] ac-20
```

`ac-20` only leaves the value state on `\n` followed by `</parameter>`. A bare
`</parameter>` is an ordinary run of characters, so the value continues across
the following `\n<parameter=mode>\ntext` and closes only on the *next*
`\n</parameter>`.

The model believes `</parameter>` closes a parameter; the grammar requires
`\n</parameter>`. The disagreement is resolved silently in favour of the
grammar.

---

# The shared root cause

Neither dialect can escape its own delimiters. Given

```
a</parameter>
<parameter=mode>
text
</parameter>
```

there is no principled way to recover whether the author meant `path="a"` +
`mode="text"` or one long `path`. Both readings are consistent with the bytes.
Any parser is choosing a heuristic, not deriving an answer — which is why this
report proposes no patch that hard-codes one reading.

## Possible directions

Trade-offs rather than a recommendation, since the choice belongs to
maintainers:

1. **Do not 500 on a parse failure.** Return the generated text as assistant
   `content` with no `tool_calls`, or at minimum a 4xx that attributes the
   failure to model output. Contained, and independent of the escaping
   question. Would address Part 1 on its own.
2. **Detect and reject rather than corrupt.** If a captured value contains the
   parameter opener or function opener, treat the call as malformed instead of
   returning it. Does not recover intent, but converts Part 2's silent wrong
   answer into a loud one.
3. **Accept a bare `</parameter>` as a close.** `path` would come back
   truncated to `"a"` — a visible failure rather than a corrupted value, at the
   cost of making such values unrepresentable.
4. **Add escaping to the dialects.** The only option that makes the value space
   complete, and the most invasive; needs template-side agreement.

Options 1 and 2 are independent of each other and of the rest, and together
they would convert both failures from "wrong or dead" into "reported".

## Why this matters beyond the pathological cases

The reproducers use deliberately hostile filenames, which are rare. The same
mechanism fires on any value that legitimately contains the delimiters —
quoted XML or JSON, HTML snippets, code containing the literal string, a diff
passed to a patch tool, a regex, a log line. Agentic workloads pass file
contents and code fragments as tool arguments routinely, and there the
corruption lands in an argument a downstream tool then acts on, or the 500
kills a session mid-task.
