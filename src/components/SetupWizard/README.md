# SetupWizard

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-SetupWizard-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-SetupWizard-complexity.json)

<!-- module-docs:start -->

Multi-step first-run setup wizard: welcome, models directory configuration, llama.cpp binary installation (with live streaming install output), Python helper setup, and completion. Driven by a step-state machine.

## Key Files

| File | Role |
|------|------|
| `SetupWizard.tsx` | Step state machine; streams llama install output; calls settings/setup APIs |

## Step Flow

```
welcome → models-dir → llama-install → python-setup → complete
```

`streamLlamaInstall()` produces a live text stream displayed in a terminal-style output area within the wizard step.

<!-- module-docs:end -->
