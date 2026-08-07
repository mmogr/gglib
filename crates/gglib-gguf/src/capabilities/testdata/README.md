# testdata

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-capabilities-testdata-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-capabilities-testdata-complexity.json)

<!-- module-docs:start -->

Fixture chat templates for `template_probe` tests, modeled on the
upstream templates of each model family — reduced to the message loop and
tool-call emission logic the probe exercises, with the exact markup
shapes the real templates render.

- `qwen2_5.jinja` / `qwen3.jinja` / `hermes2_pro.jinja` — families whose
  templates render the `<tool_call>{json}</tool_call>` envelope; the
  probe must derive the built-in Qwen spec from them.
- `llama3_1.jinja` — bare JSON with a `"parameters"` key; the probe's
  conservative rule must refuse it.
- `deepseek_r1.jinja` — fenced ```json body with the function name
  outside the JSON; likewise refused.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
<!-- module-table:end -->

</details>
