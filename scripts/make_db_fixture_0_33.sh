#!/usr/bin/env bash
# Make crates/gglib-db/tests/fixtures/library-0.33.0.sqlite: a library that
# released gglib 0.33.0 wrote, plus what its CLI cannot write offline (chats,
# benchmarks, loop-guard counts, a model file row, two model columns), put in
# by hand in the shape 0.33 stores.
#
#   scripts/make_db_fixture_0_33.sh [out file]
#
# Run on Apple silicon, from the repository root, into a path of your own:
# the checked-in file is never regenerated (crates/gglib-db/tests/fixtures/README.md).
set -euo pipefail

out=${1:?usage: $0 <out.sqlite>}
tarball=gglib-0.33.0-aarch64-apple-darwin.tar.gz

# The library lives at a neutral path, so no user name lands in a stored path.
data=/tmp/gglib-fixture
models=$data/models

# The release, alone in a new empty directory; only its gglib binary is run.
release=$(mktemp -d)
trap 'rm -rf "$release" "$data"' EXIT
gh release download v0.33.0 -R mmogr/gglib -D "$release" -p "$tarball" -p SHA256SUMS
(cd "$release" && grep " $tarball\$" SHA256SUMS | shasum -a 256 -c -)
mkdir "$release/unpacked"
tar -xzf "$release/$tarball" -C "$release/unpacked"
gglib=$release/unpacked/gglib

rm -rf "$data"
mkdir -p "$models"
export GGLIB_DATA_DIR=$data GGLIB_MODELS_DIR=$models GGLIB_DAEMON_PORT=19899
"$gglib" --version

# Three GGUF v3 files, headers only but for two tiny tensors on the image
# model, which is how its family is read.
python3 -I - "$models" <<'PY'
import struct, sys

def text(s):
    b = s.encode()
    return struct.pack("<Q", len(b)) + b

def pair(key, value):
    if isinstance(value, str):
        return text(key) + struct.pack("<I", 8) + text(value)
    return text(key) + struct.pack("<II", 4, value)  # u32

def gguf(path, pairs, tensors=()):
    head = b"GGUF" + struct.pack("<IQQ", 3, len(tensors), len(pairs))
    head += b"".join(pair(k, v) for k, v in pairs)
    offset, data = 0, b""
    for name, dims in tensors:  # f32, innermost dimension first
        head += text(name) + struct.pack("<I", len(dims))
        head += b"".join(struct.pack("<Q", d) for d in dims)
        head += struct.pack("<IQ", 0, offset)
        size = 4
        for d in dims:
            size *= d
        data += bytes(size) + bytes(-size % 32)
        offset += size + (-size % 32)
    if tensors:
        head += bytes(-len(head) % 32)
    with open(path, "wb") as f:
        f.write(head + data)

template = "{% for m in messages %}<think>{{ m.content }}</think><tool_call>{}</tool_call>{% endfor %}"
gguf(sys.argv[1] + "/fixture-agent.gguf", [
    ("general.architecture", "qwen3"),
    ("general.name", "Fixture Agent 0.6B"),
    ("general.file_type", 15),
    ("qwen3.context_length", 32768),
    ("tokenizer.chat_template", template),
])
gguf(sys.argv[1] + "/fixture-plain.gguf", [
    ("general.architecture", "llama"),
    ("general.name", "Fixture Plain"),
    ("general.file_type", 7),
    ("llama.context_length", 8192),
])
gguf(sys.argv[1] + "/fixture-image.gguf", [("general.name", "Fixture Image")],
     [("txt_in.text_norm.weight", [8]), ("img_in.weight", [64, 1])])
PY

# What 0.33.0's own code writes. `model add` asks for a parameter count.
echo 0.6 | "$gglib" model add "$models/fixture-agent.gguf"
echo 8 | "$gglib" model add "$models/fixture-plain.gguf"
"$gglib" model add "$models/fixture-image.gguf" </dev/null
"$gglib" model update 1 -y --temperature 0.6 --top-p 0.95 --top-k 20 --min-p 0 \
  --presence-penalty 1.5 --reasoning-effort high --jinja on --mtp off
"$gglib" model update 2 -y --metadata fixture.note=kept --max-tokens 512
"$gglib" model capabilities 2 --set supports-tool-calls
"$gglib" config default 1
"$gglib" config profile set fixture --description "A fixture profile" --temperature 0.2 --top-k 10
"$gglib" config settings set --default-context-size 16384 --proxy-port 18080 \
  --max-tool-iterations 12 --loop-guard-mode refuse --trust-client-sampling true \
  --proxy-allowed-hosts fixture.lan
"$gglib" mcp add --name fixture-tools --type stdio --command /bin/echo --args hello,world \
  --env FIXTURE_TOKEN=dummy --path-extra /opt/fixture/bin --lifecycle eager
"$gglib" mcp add --name fixture-off --type stdio --command /bin/cat --working-dir /tmp
"$gglib" mcp allow-remote fixture-tools
"$gglib" mcp disable fixture-off
"$gglib" daemon stop || true

# By hand, in the shape 0.33.0 stores: what it writes only with a model
# running or from a Hub download. The settings, caps, tune, applied and report
# JSON are what serde makes of 0.33.0's types (unchanged between v0.33.0 and
# the commit this ran on); the message metadata uses the keys 0.33.0 writes.
db=$data/data/gglib.db
python3 -I - "$data/pixel.png" <<'PY'
import struct, sys, zlib
def chunk(kind, body):
    return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body))
png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0))
png += chunk(b"IDAT", zlib.compress(b"\x00\x80\x80\x80")) + chunk(b"IEND", b"")
open(sys.argv[1], "wb").write(png)
PY
png_id=$(shasum -a 256 "$data/pixel.png" | cut -c1-64)
sqlite3 -bail "$db" "INSERT INTO attachments (id, mime, width, height, data, created_at)
  VALUES ('$png_id', 'image/png', 1, 1, readfile('$data/pixel.png'), '2026-11-12 01:30:00')"
sqlite3 -bail "$db" <<'SQL'
PRAGMA foreign_keys = ON;
BEGIN;
UPDATE models SET template_caps = '{"supports_tools":true,"supports_tool_calls":true,"supports_system_role":true,"supports_parallel_tool_calls":false,"supports_preserve_reasoning":null,"supports_reasoning_effort":true,"supports_string_content":true,"supports_typed_content":false,"supports_object_arguments":null,"reasoning_effort_levels":null}' WHERE id = 1;
UPDATE models SET defaults_origin = 'published' WHERE id = 2;
INSERT INTO model_files (model_id, file_path, file_index, expected_size, hf_oid, last_verified_at)
  VALUES (1, '/private/tmp/gglib-fixture/models/fixture-agent.gguf', 0, 321,
          '6e1b0c9f4d2a8e7b5c3f1a0d9e8b7c6a5f4e3d2c1b0a99887766554433221100', '2026-11-12T01:30:00+00:00');

INSERT INTO chat_conversations (id, title, model_id, system_prompt, created_at, updated_at, settings)
  VALUES (1, 'Fixture chat', 1, 'Be brief.', '2026-11-12 01:30:00', '2026-11-12 01:31:00', '{"model_name":"Fixture Agent 0.6B","model":{"machine":{"kind":"local"},"id":1},"temperature":0.6,"top_k":20,"max_tokens":1024,"profile":"fixture","ctx_size":"16384","tools":["fixture-tools:echo"],"tool_timeout_ms":30000,"max_parallel":2,"max_iterations":12,"thinking":"off"}');
INSERT INTO chat_messages (id, conversation_id, role, content, metadata, created_at) VALUES
  (1, 1, 'system', 'Be brief.', NULL, '2026-11-12 01:30:00'),
  (2, 1, 'user', 'What is in this picture?', NULL, '2026-11-12 01:30:01'),
  (3, 1, 'assistant', '', '{"tool_calls":[{"id":"call_1","name":"fixture-tools:echo","arguments":{"text":"hello"},"display_name":"echo","server":"fixture-tools"}]}', '2026-11-12 01:30:02'),
  (4, 1, 'tool', 'hello', '{"tool_call_id":"call_1"}', '2026-11-12 01:30:03'),
  (5, 1, 'assistant', 'One grey pixel.', '{"modelName":"Fixture Agent 0.6B","modelQuantization":"Q4_K_M","promptTokens":64,"cachedTokens":32,"completionTokens":12,"turnDurationMs":700,"writingDurationMs":500,"tokensPerSecond":24.0,"finishReason":"stop"}', '2026-11-12 01:30:04');
INSERT INTO message_attachments (message_id, attachment_id, position)
  SELECT 2, id, 0 FROM attachments;
-- A branch of chat 1 from its user message, as `fork` makes one.
INSERT INTO chat_conversations (id, title, model_id, system_prompt, created_at, updated_at, settings, lineage_id, branch_of)
  SELECT 2, title, model_id, system_prompt, '2026-11-12 01:32:00', '2026-11-12 01:32:01', settings, 1, 1
  FROM chat_conversations WHERE id = 1;
INSERT INTO chat_messages (id, conversation_id, role, content, metadata, created_at, origin_id) VALUES
  (6, 2, 'user', 'What is in this picture?', NULL, '2026-11-12 01:30:01', 2),
  (7, 2, 'user', 'And its colour?', NULL, '2026-11-12 01:32:01', NULL);
INSERT INTO message_attachments (message_id, attachment_id, position)
  SELECT 6, id, 0 FROM attachments;

INSERT INTO benchmark_runs (id, run_type, status, model_ids, prompt_text, system_prompt, config_json, applied_json, error, created_at, completed_at) VALUES
  (1, 'perf', 'complete', '[1]', NULL, NULL, NULL, NULL, NULL, '2026-11-12 01:40:00', '2026-11-12 01:41:00'),
  (2, 'compare', 'complete', '[1,2]', 'Say hi.', 'Be brief.', NULL, NULL, NULL, '2026-11-12 01:42:00', '2026-11-12 01:43:00'),
  (3, 'tune', 'complete', '[1]', NULL, NULL, NULL, '{"verdict":{"verdict":"incumbent_stands","incumbent_mean":0.71},"applied_config":null,"prior_defaults":null,"prior_origin":null}', NULL, '2026-11-12 01:44:00', '2026-11-12 01:45:00'),
  (4, 'agentic', 'complete', '[1]', NULL, NULL, NULL, NULL, NULL, '2026-11-12 01:46:00', '2026-11-12 01:47:00'),
  (5, 'perf', 'failed', '[2]', NULL, NULL, NULL, NULL, 'Process terminated unexpectedly', '2026-11-12 01:48:00', NULL);
INSERT INTO model_perf_results (model_id, run_id, pp_tps, tg_tps, pp_tokens, tg_tokens, backend, ngl, context_size, repetitions, created_at)
  VALUES (1, 1, 812.5, 52.5, 512, 128, 'Metal', 99, 4096, 3, '2026-11-12 01:41:00');
INSERT INTO model_compare_results (model_id, run_id, prompt_text, system_prompt, response_text, was_truncated, prompt_tokens, completion_tokens, prompt_ms, generation_ms, prompt_tps, generation_tps, created_at) VALUES
  (1, 2, 'Say hi.', 'Be brief.', 'Hi.', 0, 12, 3, 15.0, 60.0, 800.0, 50.0, '2026-11-12 01:43:00'),
  (2, 2, 'Say hi.', 'Be brief.', 'Hello there, how', 1, 12, 4, 20.0, 100.0, 600.0, 40.0, '2026-11-12 01:43:00');
INSERT INTO model_benchmark_summaries (model_id, best_tg_tps, best_pp_tps, latest_tg_tps, latest_pp_tps, latest_backend, perf_run_count, compare_run_count, last_benchmarked_at, updated_at)
  VALUES (1, 52.5, 812.5, 52.5, 812.5, 'Metal', 1, 1, '2026-11-12 01:43:00', '2026-11-12 01:43:00');
INSERT INTO benchmark_tune_results (model_id, run_id, config_json, source_json, composite_score, pruned, tg_tps, task_results_json, created_at) VALUES
  (1, 3, '{"temperature":null,"topP":null,"topK":null,"maxTokens":null,"repeatPenalty":null,"presencePenalty":null,"frequencyPenalty":null,"minP":null,"dynatempRange":null,"dynatempExponent":null,"topNSigma":null,"dryMultiplier":null,"dryBase":null,"dryAllowedLength":null,"dryPenaltyLastN":null,"seed":null,"reasoningEffort":null,"reasoningBudgetTokens":null}', '{"kind":"incumbent"}', 0.71, 0, 52.5, '[{"task_id":"single_weather","category":"single_call","passed":true,"tool_match_score":1.0,"loop_detected":false,"stagnation_detected":false,"iterations":2,"latency_ms":1840,"completion_tokens":96,"time_to_first_tool_call_ms":910,"detail":null,"unmeasured":null,"transport_retries":0,"generated":{"reasoning_chars":0,"answer_chars":0,"llm_calls":0,"max_tool_calls_in_batch":0,"system_warnings":0}}]', '2026-11-12 01:45:00'),
  (1, 3, '{"temperature":0.6,"topP":null,"topK":20,"maxTokens":null,"repeatPenalty":null,"presencePenalty":null,"frequencyPenalty":null,"minP":null,"dynatempRange":null,"dynatempExponent":null,"topNSigma":null,"dryMultiplier":null,"dryBase":null,"dryAllowedLength":null,"dryPenaltyLastN":null,"seed":null,"reasoningEffort":null,"reasoningBudgetTokens":null}', '{"kind":"family_preset","family":"qwen"}', 0.64, 1, NULL, '[{"task_id":"single_weather","category":"single_call","passed":false,"tool_match_score":0.5,"loop_detected":false,"stagnation_detected":false,"iterations":2,"latency_ms":1840,"completion_tokens":96,"time_to_first_tool_call_ms":910,"detail":null,"unmeasured":null,"transport_retries":0,"generated":{"reasoning_chars":0,"answer_chars":0,"llm_calls":0,"max_tool_calls_in_batch":0,"system_warnings":0}}]', '2026-11-12 01:45:00');
INSERT INTO benchmark_agentic_results (model_id, run_id, raw_composite, gglib_composite, report_json, created_at)
  VALUES (1, 4, 0.62, 0.81, '{"model_name":"Fixture Agent 0.6B","quantization":"Q4_K_M","param_count_b":0.6,"ctx_size":16384,"raw":{"tool_accuracy":0.75,"loop_avoidance":1.0,"loop_eligible":1,"task_completion":0.5,"composite":0.62,"tg_tps":52.5,"total_completion_tokens":192,"total_wall_ms":3680,"measured_wall_ms":3680,"mean_time_to_first_tool_call_ms":910.0,"median_time_to_first_tool_call_ms":910.0,"seeds":1,"runs":1,"unmeasured_runs":0,"transport_retries":0,"generated":{"reasoning_chars":0,"answer_chars":0,"llm_calls":0,"max_tool_calls_in_batch":0,"system_warnings":0}},"gglib":{"tool_accuracy":0.75,"loop_avoidance":1.0,"loop_eligible":1,"task_completion":0.5,"composite":0.81,"tg_tps":52.5,"total_completion_tokens":192,"total_wall_ms":3680,"measured_wall_ms":3680,"mean_time_to_first_tool_call_ms":910.0,"median_time_to_first_tool_call_ms":910.0,"seeds":1,"runs":1,"unmeasured_runs":0,"transport_retries":0,"generated":{"reasoning_chars":0,"answer_chars":0,"llm_calls":0,"max_tool_calls_in_batch":0,"system_warnings":0}},"delta":{"tool_accuracy":0.0,"loop_avoidance":0.0,"task_completion":0.0,"composite":0.19,"withheld":null,"wall_time_speedup":1.0,"completion_token_ratio":1.0},"tasks":[{"task_id":"single_weather","category":"single_call","raw":[{"task_id":"single_weather","category":"single_call","passed":false,"tool_match_score":0.5,"loop_detected":false,"stagnation_detected":false,"iterations":2,"latency_ms":1840,"completion_tokens":96,"time_to_first_tool_call_ms":910,"detail":null,"unmeasured":null,"transport_retries":0,"generated":{"reasoning_chars":0,"answer_chars":0,"llm_calls":0,"max_tool_calls_in_batch":0,"system_warnings":0}}],"gglib":[{"task_id":"single_weather","category":"single_call","passed":true,"tool_match_score":1.0,"loop_detected":false,"stagnation_detected":false,"iterations":2,"latency_ms":1840,"completion_tokens":96,"time_to_first_tool_call_ms":910,"detail":null,"unmeasured":null,"transport_retries":0,"generated":{"reasoning_chars":0,"answer_chars":0,"llm_calls":0,"max_tool_calls_in_batch":0,"system_warnings":0}}]}],"seeds":[7],"control":null,"raw_replicate":null,"replicate_seeds":[],"raw_replicates":[],"replicate_seed_sets":[],"paired":null,"proxy":null}', '2026-11-12 01:47:00');

-- Loop-guard counts on 2026-11-12 (epoch day 20769) and the day before.
INSERT INTO loop_guard_trips (recorded_at, model_name, gglib_version, mode, detector, signature_hash, session_hash, repeat_count, threshold) VALUES
  (1794445200, 'Fixture Agent 0.6B', '0.33.0', 'refuse', 'loop', '9f2c4e1a7b3d5f60', 'a1b2c3d4e5f60718', 3, 3),
  (1794448800, 'Fixture Agent 0.6B', '0.33.0', 'refuse', 'stagnation', '0c1d2e3f4a5b6c7d', 'a1b2c3d4e5f60718', 4, 4);
INSERT INTO loop_guard_scans (day, model_name, gglib_version, mode, scanned) VALUES
  (20769, 'Fixture Agent 0.6B', '0.33.0', 'refuse', 40),
  (20768, 'Fixture Plain', '0.33.0', 'note', 7);
COMMIT;
PRAGMA foreign_key_check;
SQL

# One file, compacted, with nothing of this machine in it. Out of WAL mode,
# so opening it leaves no -wal or -shm file beside it; gglib turns WAL on again.
sqlite3 -bail "$db" 'PRAGMA wal_checkpoint(TRUNCATE);'
made=$release/library.sqlite
cp "$db" "$made"
sqlite3 -bail "$made" 'PRAGMA journal_mode = DELETE;' 'VACUUM;' >/dev/null
[ "$(sqlite3 "$made" 'PRAGMA integrity_check;')" = ok ]
rm -f "$made-shm" "$made-wal"
# Checked before it reaches $out.
if sqlite3 "$made" .dump | grep -iF -e "$HOME" -e "$USER" -e /Users/ -e /home/; then
  echo "a stored text names this machine's user" >&2
  exit 1
fi
mv "$made" "$out"
ls -l "$out"
