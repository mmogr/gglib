# SetupWizard

<!-- module-docs:start -->

Multi-step first-run setup wizard: welcome, models directory configuration, both engines' installation (with live install progress), Python helper setup, and completion. Driven by a step-state machine.

## Key Files

| File | Role |
|------|------|
| `SetupWizard.tsx` | Step state machine; calls settings/setup APIs |
| `EnginesStep.tsx` | The engines step: reads each engine's status and installs every missing one whose install is a download, llama.cpp first, then stable-diffusion.cpp. llama.cpp's failure is an error, stable-diffusion.cpp's a warning; a source build or a machine with no download is sent to the terminal |
| `InstallProgress.tsx` | Renders one pre-built install event — a bar for the download phase, a labelled spinner for every other, the download named for its `product`. `LlamaInstallModal` and the Settings engine card draw their installs with it too |

## Step Flow

```
welcome → models-dir → engines → python-setup → complete
```

`streamEngineInstall(kind, …)` streams each install's events, drawn by `InstallProgress` within the engines step.

<!-- module-docs:end -->
