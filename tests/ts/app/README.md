# app

Tests of the main window's shell pieces that are not places: the confirm store, the paths, the router's creation, the native-menu bridge and the chat routes (the switch's cases are the library page test's).

| File | Pins |
|------|------|
| `confirm.test.tsx` | A question asked outside React is drawn by the provider and answered by its buttons; the context hands components the same confirm; a second question while one is open is answered false; a move through history answers the open one false |
| `paths.test.ts` | Each path's shape and `inspectorSubject` reading it back; `menuModelId`, the inspector's model or the open chat's; the settings dialog as a search parameter over the place; the menu's interim intents; the benchmark model ids |
| `router.test.tsx` | The router is created after the daemon's token has left the address, so its first location carries no token; created twice in the browser it is one router; `/` goes to the library; an unknown address does too |
| `menuBridge.test.tsx` | Each menu event's effect: Settings opens over the place, Add Model and Downloads open the library's Add pane, Chat goes to the selected model's server else the first running else says none, Refresh refreshes the library, Start opens the picked model's Serve dialog, Stop stops the selected running model and nothing with another picked, on a chat the menu's model is the chat's and Stop leaves it for its inspector, Remove asks first and goes back to the library; the menu is told the selected model on every move, once per change |
| `chatRoutes.test.tsx` | `/chat/:modelId` goes to the model's inspector when nothing serves it; a far chat opens on the paired machine's model, keeps across a disconnection and a connection that names no machine yet, and closes once the status names another machine |
