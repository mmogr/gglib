# Screenshots

The GUI pictures in `docs/assets/` (`gui-library.png`, `gui-chat.png`,
`gui-console.png`, `tray-dashboard.png`) are taken by `shot.mjs` here, so
they are pictures of gglib as it is rather than as it once was. Nothing
here runs in CI; run it when the GUI changes, and commit the result.

It needs Node 20+ and Google Chrome; `npm install` in this folder fetches
puppeteer-core. No browser is downloaded: it drives the Chrome at
`/Applications/Google Chrome.app` (set `CHROME` for another path).

The terminal pictures (`cli-up.gif`, `cli-up.png`, `cli-remote.png`) were
recorded from a pty and rendered with xterm.js; redo them with any terminal
recorder when the commands' output changes. `social-preview.png` is the card
GitHub shows when the repo is linked, made by hand.

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
