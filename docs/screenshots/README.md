# Screenshots

The GUI pictures in `docs/assets/` (`gui-library.png`, `gui-chat.png`,
`gui-console.png`, `tray-dashboard.png`) are taken by `shot.mjs` here, so
they are pictures of gglib as it is rather than as it once was. Nothing
here runs in CI; run it when the GUI changes, and commit the result.

It needs Node 20+ and Google Chrome; `npm install` in this folder fetches
puppeteer-core, and xterm.js for the terminal GIFs. No browser is
downloaded: it drives the Chrome at `/Applications/Google Chrome.app` (set
`CHROME` for another path).

The terminal GIFs (`cli-up.gif`, `cli-opencode.gif`) are recorded by
`record.py` and drawn by `render.mjs`; see [Terminal GIFs](#terminal-gifs).
The stills `cli-up.png` and `cli-remote.png` came before these scripts:
recorded from a pty and rendered with xterm.js by hand. `social-preview.png`
is the card GitHub shows when the repo is linked, made by hand.

## Against a copy, not your install

Take the shots from a gglib with its own data folder and port, so your
library and settings stay out of the picture and nothing you do for a photo
changes them:

```bash
export GGLIB_DATA_DIR=$HOME/gglib-shots GGLIB_DAEMON_PORT=9897
gglib model add <a .gguf file>             # once per model; a library worth looking at
gglib daemon run &                         # then start the proxy from the GUI, or:
curl -s -X POST -H "Authorization: Bearer $(cat $GGLIB_DATA_DIR/data/daemon_token)" \
  -H 'content-type: application/json' -d '{"port":8181}' \
  http://127.0.0.1:$GGLIB_DAEMON_PORT/api/proxy/start
URL=$(gglib web --no-open 2>&1 | grep -o 'http://[^ ]*')   # the link carries the daemon token
```

The chat and console shots need a model loaded and a conversation held, so
send a few turns through the chat page first. A title the model generates
can be renamed in the sidebar before the shot. The pictures show whatever
paths the copy prints, your models folder among them.

## Taking them

```bash
node shot.mjs "$URL" ../assets/gui-library.png steps/library.json
node shot.mjs "$URL" ../assets/gui-chat.png    steps/chat.json
node shot.mjs "$URL" ../assets/gui-console.png steps/console.json
W=360 H=560 node shot.mjs "${URL/\/#/\/tray.html#}" ../assets/tray-dashboard.png
```

1440×900 at 2× unless `W` and `H` say otherwise; `SCHEME=light` for the
light theme. A steps file is a list of actions run before the screenshot:
`click` (a CSS selector), `clickText` (an element whose text is exactly
this), `type`, `press`, `waitText`, `sleep`. Edit the model names in
`steps/*.json` to match the library you built. `DUMP=out.txt` also writes
the page's visible text, which is how to find out what to click next.

## Terminal GIFs

`record.py` runs a script of keystrokes against a bash in a pty and writes
what it printed, with timings, as an asciicast. `render.mjs` plays that into
xterm.js in the same headless Chrome and has ffmpeg (`brew install ffmpeg`)
make the GIF. Pauses are cut to `--idle` seconds, so a download or a model
load reads as a beat. The scripts are `steps/cli-up.json` and
`steps/cli-opencode.json`; `record.py` says what a step can be, including the
chapter cards, the fast-forward and the side pane the opencode one uses.

Both run against a copy of their own in `~/.local/share/gglib-demo` and in
`orders-api/` here, a small project whose export leaks database connections.
Copy it to `~/.local/share/gglib-demo/home/orders-api`, then `git init`, add
and commit it, so the last chapter's `git diff` has something to compare
against. The scripts set `HOME` and opencode's `XDG_*` folders under that
copy, so neither the recording nor opencode touches your own setup. The copy
serves on 8080 so the pictures show the default port, which means stopping
your own proxy (`gglib proxy stop`) while you record:

```bash
export GGLIB_DATA_DIR=$HOME/.local/share/gglib-demo GGLIB_DAEMON_PORT=9907
export GGLIB_MODELS_DIR=$GGLIB_DATA_DIR/models
gglib config settings set --proxy-port 8080 --llama-base-port 9300
gglib model add $GGLIB_MODELS_DIR/<the model to serve>.gguf
```

```bash
python3 record.py steps/cli-up.json up.cast
node render.mjs up.cast ../assets/cli-up.gif --idle 1 --hold 5 --rename "$HOME=/Users/mmogr"
```

`cli-up.gif` wants an empty copy holding only the model to serve
(`gglib model add`), so `gglib up` installs the engines on camera.
`cli-opencode.gif` comes after it, with two more models added and
`orders-api/opencode.json` naming them. Its lower pane follows the daemon's
log, so start the proxy with nothing loaded; the model's launch card then
lands in that pane when opencode's first request does:

```bash
gglib daemon stop && gglib web --no-open   # a fresh daemon, nothing loaded
curl -s -X POST -H "Authorization: Bearer $(cat $GGLIB_DATA_DIR/data/daemon_token)" \
  -H 'content-type: application/json' -d '{}' \
  http://127.0.0.1:$GGLIB_DAEMON_PORT/api/proxy/start   # on the stored proxy_port
python3 record.py steps/cli-opencode.json opencode.cast
node render.mjs opencode.cast ../assets/cli-opencode.gif --idle 1 --hold 6 --rename "$HOME=/Users/mmogr"
```

`--rename` swaps text before it is drawn, so the paths in the picture do not
name you.
