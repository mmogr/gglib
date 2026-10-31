# gglib image

Draw a picture with this machine's image model, through the daemon (it
starts one if none runs). The render is shown on one line as it goes, each
PNG is saved, and its line is printed: `[image WxH] <path>`.

```bash
gglib image "a lighthouse at dusk, oil painting"
gglib image "a red fox in snow" --size 1024x768 -n 2 -o fox.png
gglib image "a red fox in snow" --from fox.png --strength 0.4 -o fox-again.png
gglib image "a fox's paw in snow" --from fox.png --mask paw.png -o fox-fixed.png
```

An image model is a library entry like any other, with companion files
beside its weights: [`gglib model`](model.md#image-models) says which
families draw and how components are linked, and `gglib config sd` installs
the runtime that draws them ([config](config.md#llama-and-sd)).

## The model, the size, the seed

- The model is the settings' default image model, else the only one with
  every file its family needs, unless `-m` names one.
- The size is 1024x1024 unless `--size` gives another the family draws.
  `-n` draws up to four; `--seed` repeats a drawing.
- The line shows the render's state: queued, with its place; loading;
  sampling, step of total, and image of images for more than one; decoding.
  The first step of a Flux.1 render comes about 40 seconds in.

## From a picture

- `--from <PNG>` draws from a picture in place of starting from nothing.
  The result is the picture's size, so `--size` is refused beside it, and
  the size must be one the model's family draws. `--strength`, from 0.05 to
  1 and 0.6 when left out, says how far from the picture it may go.
- With `--mask <PNG>` as well, a PNG the picture's size, only what is white
  in the mask is drawn again: every pixel more than 16 pixels from the white
  is the picture's own, unchanged.

## Where the files go

- `-o PATH`, or `gglib-<unix time>.png` in the current folder, with `-1`,
  `-2`, … for more than one. An existing file is never replaced.

## Stopping

- Ctrl-C leaves at once, and the daemon stops the render. One that is
  already generating is stopped by stopping `sd-server`, so that picture is
  lost, the chats that waited go on, and the next picture loads the image
  model again.
- Stopping the image model from the app, or removing it there with force,
  while it draws waits up to 20 seconds for the render to stop, and answers
  an error if it has not, leaving the model loaded. The command line's
  `model remove --yes` skips the confirmation and stops nothing.

In a chat, `/draw` asks the model to draw for the next message
([chat](chat.md#draw)).

## Flags

`gglib image --help` lists them all.
