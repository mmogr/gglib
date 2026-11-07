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
| `SetupPlace.tsx` | The column, the setup status the gate read (not read again), the one read of the recommendation ("Can't reach gglib." with Try again), the step switch, the endpoint turned on once on step 4, the proof once the model is here, "Already done" moving on once, and Open gglib writing `setupCompleted` and landing on `/models/:id` (the Library's Hugging Face search after Choose another) |
| `Step.tsx` | The frame every step is drawn in, the item line (name, state, what is said under it), an error or a warning under it with Retry, and `useAdvance` for "Already done" |
| `ThisMacStep.tsx` | Step 1: the machine and its memory (the recommendation's budget, else the machine's RAM), the models folder with "(will be created)", Change saving through the models-directory route and showing the folder read back |
| `EnginesStep.tsx` | Step 2: llama.cpp installing then ready, an error with Retry, or the source build's command with Check again; stable-diffusion.cpp ready or skipped, a warning; faster downloads set up, to be set up with the first model, or needing Python 3 |
| `useEngines.ts` | Step 2's state: the engines read and installed on the first arrival, llama.cpp first and stable-diffusion.cpp after it whatever llama.cpp did, a stream that closes without a result ended with a reason, a source build looked for every few seconds until found; leaving setup stops reading the streams and starts no further install |
| `useFastDownloads.ts` | The download accelerator: set up, unasked, when Get it is pressed and before the download is queued, as `gglib up` does once a download is agreed; never where Python 3 is not there; a failure a warning with Retry, the download going ahead |
| `FirstModelStep.tsx` | Step 3: the suggestion in the route's words, Get it (faster downloads set up first) with the download's row, Choose another, Skip, and the pairing field for another machine's models |
| `useFirstModel.ts` | Step 3's state: the choice, the download by the id the queue answered with (or, when the queue refuses the repo as already queued, the repo's row it has), and the library's model from that repository |
| `EndpointStep.tsx` | Step 4: the endpoint's line with Copy, the proof or what it waits for, a failed start or proof with Retry, Start the endpoint when gglib launches (`proxyAutostart`) |
| `ConnectStep.tsx` | Step 5: Connect a client with the first model's Model ID, and Open gglib |

## Words said only here

GUI words with no Rust source, kept in one list so they can be moved to
the daemon if a second surface needs them: the headlines (This Mac,
Engines, First model, Endpoint, Connect); "n of 5"; "(will be created)",
"(not writable)"; the engine states
"reading", "installing", "ready", "did not install", "needs building",
"waiting", "skipped — chat works without it"; "This Mac needs llama.cpp
built from source. In a terminal:", "gglib looks again every few
seconds."; "Faster downloads", "set up", "set up when you get a model",
"setting up", "needs Python 3; downloads still work", "not set up;
downloads still work", "setting up faster downloads first"; "Suggested",
"Nothing gglib would suggest fits."; the suggestion's "Needs … of …",
moved unchanged from the Library's (`formatRecommendation`); "system RAM"
after the machine's RAM is Rust's `BudgetSource::SystemRam` label; "Use another machine's models ›",
"Joined. Its models are in your Library."; "The test runs when … finishes.",
"Testing…", "Start the endpoint when gglib launches"; "Clients connect
once the endpoint is on: turn it on in Endpoint."; "Already done";
"Can't reach gglib."; the buttons Continue, Back, Skip, Change, Save,
Cancel, Retry, Check again, Get it, Choose another, Join, Copy, Try again,
Open gglib.

<!-- module-docs:end -->
