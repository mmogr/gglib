# Toast

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-Toast-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-Toast-complexity.json)

<!-- module-docs:start -->

Toast notification system with auto-dismiss, pause-on-hover, keyboard-dismiss support, and enter/exit CSS animations. Supports `success`, `error`, `info`, and `warning` severity types.

## Key Files

| File | Role |
|------|------|
| `Toast.tsx` | `ToastContainer` renders the stack; `ToastItem` manages individual toast lifecycle |

## Lifecycle

```
Enqueued → auto-dismiss timer starts
         → user hovers: timer pauses
         → user leaves: timer resumes
         → timer expires or × clicked: exit animation → unmount
```

Driven entirely by `toasts[]` prop and `onDismiss` callback — no internal queue state.

<!-- module-docs:end -->
