# setup

<!-- module-docs:start -->

Setup: five steps in one centred 560 px column, the window's only content
while `app/SetupGate` holds it (the daemon's `setupCompleted` is false).
Each step says "n of 5" in 12 px and its 22 px headline, with Back as text
and Continue (or Get it, or Open gglib) the one ink button; no stepper, no
sidebar. Setup is not a route, and nothing opens it again: later problems
are the Library's banner, Settings › Engines and the Endpoint place.

Every fact is the daemon's: the machine from the setup status (the
download llama.cpp's install would take) and the recommendation (the
memory it was sized against, in `gglib up`'s words); the engines from
their status route and their install stream, the one Settings and the
Library banner read (SU-1); the suggestion from the recommend route; the
download from its row's own text (TX-13); the endpoint from
`proxyRegistry`; the proof in `readings/connect`'s words; the recipes from
`components/ConnectClient`. Everything a step starts is held by the place,
so Back starts nothing twice.

| File | Role |
|------|------|
| `SetupPlace.tsx` | The column, the one read of the setup status and the recommendation ("Can't reach gglib." with Try again), the step switch, the endpoint turned on once on step 4, the proof once the model is here, "Already done" moving on once, and Open gglib writing `setupCompleted` and landing on `/models/:id` (the Library's Hugging Face search after Choose another) |
| `Step.tsx` | The frame every step is drawn in, the item line (name, state, what is said under it), an error or a warning under it with Retry, and `useAdvance` for "Already done" |
| `ThisMacStep.tsx` | Step 1: the machine and its memory, the models folder with "(will be created)", Change saving through the models-directory route and showing the folder read back |
| `EnginesStep.tsx` | Step 2: llama.cpp installing then ready, an error with Retry, or the source build's command with Check again; stable-diffusion.cpp ready or skipped, a warning; faster downloads set up, or needing Python 3 |
| `useEngines.ts` | Step 2's state: the engines read and installed on the first arrival, llama.cpp first, a stream that closes without a result ended with a reason, a source build looked for every few seconds, the accelerator set up where Python 3 is there |
| `FirstModelStep.tsx` | Step 3: the suggestion in the route's words, Get it with the download's row, Choose another, Skip, and the pairing field for another machine's models |
| `useFirstModel.ts` | Step 3's state: the choice, the download by the id the queue answered with, and the library's model from that repository |
| `EndpointStep.tsx` | Step 4: the endpoint's line with Copy, the proof or what it waits for, a failed start or proof with Retry, Start the endpoint when gglib launches (`proxyAutostart`) |
| `ConnectStep.tsx` | Step 5: Connect a client with the first model's Model ID, and Open gglib |

## Words said only here

GUI words with no Rust source, kept in one list so they can be moved to
the daemon if a second surface needs them: the headlines (This Mac,
Engines, First model, Endpoint, Connect); "n of 5"; "(will be created)",
"(not writable)", "Set by GGLIB_MODELS_DIR."; the engine states
"reading", "installing", "ready", "did not install", "needs building",
"waiting", "skipped — chat works without it"; "This Mac needs llama.cpp
built from source. In a terminal:", "gglib looks again every few
seconds."; "Faster downloads", "set up", "setting up", "needs Python 3;
downloads still work", "not set up; downloads still work"; "Suggested",
"Nothing gglib would suggest fits."; "Use another machine's models ›",
"Joined. Its models are in your Library."; "The test runs when … finishes.",
"Testing…", "Start the endpoint when gglib launches"; "Clients connect
once the endpoint is on: turn it on in Endpoint."; "Already done";
"Can't reach gglib."; the buttons Continue, Back, Skip, Change, Save,
Cancel, Retry, Check again, Get it, Choose another, Join, Copy, Try again,
Open gglib.

<!-- module-docs:end -->
