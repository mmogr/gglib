# SetupWizard

<!-- module-docs:start -->

Multi-step first-run setup wizard: welcome, models directory configuration, llama.cpp binary installation (with live streaming install output), Python helper setup, and completion. Driven by a step-state machine.

## Key Files

| File | Role |
|------|------|
| `SetupWizard.tsx` | Step state machine; streams llama install output; calls settings/setup APIs |
| `InstallProgress.tsx` | Renders one pre-built install event — a bar for the download phase, a labelled spinner for every other, the download named for its `product`. `LlamaInstallModal` and the image runtime section of Settings draw their installs with it too |

## Step Flow

```
welcome → models-dir → llama-install → python-setup → complete
```

`streamLlamaInstall()` produces a live text stream displayed in a terminal-style output area within the wizard step.

<!-- module-docs:end -->
