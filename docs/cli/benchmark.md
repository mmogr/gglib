# gglib benchmark

Measurement. Compare what models say, measure how fast they run, find the
sampling settings that make one good at tool calls, and measure what the
pipeline between a client and llama-server buys. Every run is kept, and
`list`, `show` and `model` read them back.

<!-- snippet: benchmark-compare -->
```console
$ gglib benchmark compare -p "Write a haiku about a lighthouse, and nothing else." -m Qwen3-4B-Instruct-2507 -m Qwen3.8-27B --max-tokens 400

  ╭─ 📊 Benchmark Compare ─────────────────╮
  Models : Qwen3-4B-Instruct-2507, Qwen3.8-27B

  ╰───────────────────────────────────────╯

[1/2] Qwen3-4B-Instruct-2507
Salt wind whispers tall,  
ancient beam cuts dark sea—  
watchers' steady eye.
✓ 84.8 tok/s gen  ·  358.0 tok/s prompt  ·  0 tokens  ·  0.2s

[2/2] Qwen3.8-27B
We need to respond to user: "Write a haiku about a lighthouse, and nothing else." Need final only haiku, nothing else. Need likely 5-7-5 syllables? Let's craft: "Stone sentinel stands" (4? Stone(1) sen(2) ti(3) nel(4) stands(5) =5). "its beam cuts the midnight sea" (its1 beam2 cuts3 the4 mid5 night6 sea7? Wait its=1, beam=1 =>2, cuts=1=>3, the=1=>4, mid=1=>5, night=1=>6, sea=1=>7 yes 7). "waves break in white foam" (waves1 break2 in3 white4 foam5 =5). Good. Final only those three lines. Ensure nothing else.
Stone sentinel stands  
its beam cuts the midnight sea  
waves break in white foam
✓ 27.9 tok/s gen  ·  103.7 tok/s prompt  ·  0 tokens  ·  7.0s

✓ Run #4 complete
```
<!-- /snippet -->

## `compare`

One prompt through N models, one after another, each output followed by
its speed: `-p` is the prompt, `-m` names each model, `-s` an optional system
prompt, and `--temperature`, `--max-tokens` and `--ctx-size` apply to every
model in the run.

## `perf`

Raw throughput with llama-bench: prompt processing over `--pp` tokens (512)
and generation over `--tg` tokens (128), averaged over `--reps` runs (3),
for each `-m`.

## `tune`

A sweep over sampling parameters for one model, scored against an agentic
tool-calling suite on both tool-call accuracy and resistance to loops and
stagnation. `--apply` writes the winner back as the model's measured
defaults, through a gate: it must clear the incumbent's mean by twice the
run's own drift, with the per-task pairs agreeing, and a refusal prints its
evidence and changes nothing.

```bash
gglib benchmark tune -m Qwen3-4B-Instruct-2507 \
  --sweep temperature=0.2,0.5,0.8 --sweep top_p=0.8,0.95 --apply
```

- `--sweep <dim>=<v1>,<v2>,…`, one per dimension (`temperature`, `top_p`,
  `top_k`, `min_p`, `repeat_penalty`, `dry_multiplier`, `dynatemp_range`,
  `dynatemp_exponent`, `top_n_sigma`). The grid is the cartesian product, so
  dimensions multiply. A dimension left unswept is not varied: the normal
  per-model, global and recipe chain fills it in, or llama.cpp's own default
  where none of them names it.
- The grid is seeded from built-in per-family presets, matched by the
  model's name (`--seed-from-family-presets false` to skip), and pruned
  after a cheap pre-screen round: `--prune-fraction` (0.5) drops that share
  of the candidates, never below three survivors.
- `--task-suite` is `default`, the built-in BFCL-style suite (single call,
  parallel call, multi-turn, irrelevance detection, long-context endurance),
  or a path to a JSON file of tasks in the same schema; the desktop app
  accepts the same file.
- The three score weights, `--weight-tool-accuracy`,
  `--weight-loop-avoidance` and `--weight-task-completion`, shape the
  composite a candidate is ranked by.

[Sampling resolution](../sampling.md) says where the written defaults sit
in the hierarchy.

## `agentic`

What the pipeline buys: the same suite run against one model with the
pipeline bypassed (bare llama-server defaults) and through it, with the
before/after delta in tool-call accuracy, loop avoidance and task
completion.

- Every task runs under each of `--seeds` (three by default), and every
  score is the mean across them: one sample per task is a draw from the
  model's output distribution, not a measurement of it. `--seeds ""` is one
  unseeded run.
- Two arms keep the result honest, and each can be skipped: the positive
  control (`--no-control`) runs the pipeline at a deliberately bad
  temperature and must score below it, or a null result cannot be told from
  a harness that measures nothing; the A/A replicate (`--no-replicate`,
  `--replicate-pairs N`) re-runs the raw arm on a disjoint seed set, and the
  gap it opens is the eval's own drift, the floor a raw-vs-gglib delta has
  to clear before it means anything.
- `--proxy` adds two arms: one through a real `gglib-proxy` started in
  front of the model, which validates each tool call against its schema and
  re-issues a broken one, and a raw baseline straight to llama-server beside
  it. They are compared with each other, not with the first two arms.
- `--json` prints the full report, `--output <file>` writes it: per-arm
  scores, deltas, a per-task drill-down, the model's identity and a hardware
  snapshot.

## `list`, `show`, `model`

Past runs. `list` shows the latest `-n` (10); `show <run-id>` one run in
full; `model <id>` every compare and perf run for one model, by its database
id.

## Flags

`gglib benchmark <subcommand> --help` lists every flag.
