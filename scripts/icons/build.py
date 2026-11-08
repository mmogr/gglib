#!/usr/bin/env python3
"""Draw gglib's icons, every platform's, from this one file.

The icon is a database (the model library) and a speech bubble holding a
llama (llama.cpp), over a blue field with the name on a cream band. Its shapes
are drawn here; the two that are not simple geometry, the llama and the
wordmark, are outlines in `shapes.json`. Every file the app ships is written
from them, so no icon can drift from another:

- `src-tauri/icons/Assets.car`, the icon macOS 26 and later show: an Icon
  Composer icon whose background, database, bubble and wordmark are separate
  layers so the system can light them, the bubble and database with its glass
  edge, compiled by `actool`. Without it macOS 26 shrinks the icon onto a grey
  tile. It is compiled here rather than at bundle time because the bundler
  takes an `.icon` only as a directory in `bundle.icon`, and the Linux
  bundler stops at a directory there; a `.car` is a file the others skip.
- `icon.icns`, `icon.ico`, the PNGs, the Windows Store logos and the iOS and
  Android sets, all from `tauri icon`: the same art inside the rounded square
  Apple's pre-26 template uses, which Windows and Linux show as it is. At 48 px
  and below the wordmark is a smudge, so those sizes (the `.ico`'s small
  entries, the `.icns`'s 16 and 32 px, `32x32.png` and the two small Store
  logos) are redrawn without it: the database and bubble alone, larger.
- `tray-idle.png` and `tray-active.png`: the bubble with the llama cut out of
  it, and the same with the dot that says a model is loaded. Black on clear,
  which macOS needs for a template image; the app tints them on Windows and
  Linux.

SVG becomes PNG through `tauri icon` (resvg), so this needs the repo's
`node_modules`, and macOS with Xcode 26 or later for `actool`. Run it after
changing a shape, and commit what it writes:

    make icons
"""

import json
import pathlib
import re
import struct
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[2]
ICONS = ROOT / "src-tauri" / "icons"
SHAPES = json.loads((pathlib.Path(__file__).parent / "shapes.json").read_text())

# --- the art, on a 1024 canvas that is the whole icon shape ------------------

BAND_Y = 690  # where the cream band starts
DB_CX, DB_RX, DB_RY, DB_H = 372, 212, 62, 112  # database disks
DB_TOPS = (150, 290, 430)
BUBBLE = (
    "M540,150 H836 Q900,150 900,214 V500 Q900,564 836,564 "
    "H640 L586,624 Q574,636 572,618 L566,564 H540 Q476,564 476,500 V214 Q476,150 540,150 Z"
)
BUBBLE_BODY = 'x="476" y="150" width="424" height="414" rx="64"'  # the bubble without its tail
LLAMA_MATRIX = "matrix(0.034 0 0 -0.034 494.8 585.68)"  # shapes.json frame -> canvas
HERO = "translate(512,392) scale(0.9) translate(-520,-380)"  # database + bubble, clear of the edge
HERO_SMALL = "translate(512,512) scale(1.12) translate(-530,-362)"  # the same, centred and larger
SMALL = (16, 24, 30, 32, 44, 48)  # px drawn without the wordmark
WORD_SCALE = 0.74

DEFS = """
<linearGradient id="sky" x1="0" y1="0" x2="0.25" y2="1">
  <stop offset="0" stop-color="#6cbcf6"/><stop offset="0.55" stop-color="#3d8be0"/><stop offset="1" stop-color="#2463c4"/>
</linearGradient>
<radialGradient id="sheen" cx="0.2" cy="0" r="0.8">
  <stop offset="0" stop-color="#ffffff" stop-opacity="0.22"/><stop offset="1" stop-color="#ffffff" stop-opacity="0"/>
</radialGradient>
<linearGradient id="cream" x1="0" y1="0" x2="0" y2="1">
  <stop offset="0" stop-color="#fbfaf6"/><stop offset="1" stop-color="#efece4"/>
</linearGradient>
<linearGradient id="dbside" x1="0" y1="0" x2="1" y2="0">
  <stop offset="0" stop-color="#7fb9ec"/><stop offset="0.3" stop-color="#b7dcfb"/>
  <stop offset="0.75" stop-color="#8cc3f1"/><stop offset="1" stop-color="#5f9fe0"/>
</linearGradient>
<linearGradient id="dblid" x1="0" y1="0" x2="1" y2="1">
  <stop offset="0" stop-color="#eaf6ff"/><stop offset="1" stop-color="#bfe0fb"/>
</linearGradient>
<linearGradient id="bubble" x1="0" y1="0" x2="0" y2="1">
  <stop offset="0" stop-color="#ffffff"/><stop offset="1" stop-color="#f1efe9"/>
</linearGradient>
<linearGradient id="llama" x1="0" y1="0" x2="0" y2="1">
  <stop offset="0" stop-color="#4b5d78"/><stop offset="1" stop-color="#34445c"/>
</linearGradient>
<filter id="soft" x="-0.3" y="-0.3" width="1.6" height="1.6"><feGaussianBlur stdDeviation="18"/></filter>
<filter id="tight" x="-0.3" y="-0.3" width="1.6" height="1.6"><feGaussianBlur stdDeviation="6"/></filter>
<filter id="drop" x="-0.1" y="-0.1" width="1.2" height="1.25"><feGaussianBlur stdDeviation="12"/></filter>
"""


def svg(body: str, size: int = 1024, defs: str = DEFS) -> str:
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {size} {size}" width="{size}" height="{size}">'
        f"<defs>{defs}</defs>{body}</svg>"
    )


def background(band: bool = True) -> str:
    sky = '<rect width="1024" height="1024" fill="url(#sky)"/><rect width="1024" height="1024" fill="url(#sheen)"/>'
    if not band:
        return sky
    return (
        sky
        + f'<rect y="{BAND_Y}" width="1024" height="{1024 - BAND_Y}" fill="url(#cream)"/>'
        f'<rect y="{BAND_Y}" width="1024" height="3" fill="#ffffff" opacity="0.9"/>'
        f'<rect y="{BAND_Y - 14}" width="1024" height="14" fill="#123c80" opacity="0.10" filter="url(#tight)"/>'
    )


def database(hero: str = HERO) -> str:
    left, right = DB_CX - DB_RX, DB_CX + DB_RX
    out = (
        f'<ellipse cx="{DB_CX + 14}" cy="{DB_TOPS[-1] + DB_H + 30}" rx="{DB_RX}" ry="{DB_RY}" '
        'fill="#0d3a85" opacity="0.35" filter="url(#soft)"/>'
    )
    for cy in reversed(DB_TOPS):
        side = f"M{left},{cy} V{cy + DB_H} A{DB_RX},{DB_RY} 0 0 0 {right},{cy + DB_H} V{cy} Z"
        out += (
            f'<path d="{side}" fill="url(#dbside)"/>'
            f'<path d="M{left + 6},{cy + DB_H} A{DB_RX - 6},{DB_RY - 6} 0 0 0 {right - 6},{cy + DB_H}" '
            'fill="none" stroke="#ffffff" stroke-opacity="0.35" stroke-width="4"/>'
            f'<ellipse cx="{DB_CX}" cy="{cy}" rx="{DB_RX}" ry="{DB_RY}" fill="url(#dblid)"/>'
            f'<ellipse cx="{DB_CX}" cy="{cy}" rx="{DB_RX - 3}" ry="{DB_RY - 3}" fill="none" '
            'stroke="#ffffff" stroke-width="5" stroke-opacity="0.9"/>'
        )
    return f'<g transform="{hero}">{out}</g>'


def llama(fill: str) -> str:
    """The llama, clipped to the bubble's body so the neck ends on its edge."""
    return (
        f'<clipPath id="inbubble"><rect {BUBBLE_BODY}/></clipPath>'
        f'<g clip-path="url(#inbubble)"><path transform="{LLAMA_MATRIX}" '
        f'd="{SHAPES["llama"]["d"]}" fill="{fill}" fill-rule="evenodd"/></g>'
    )


def bubble(hero: str = HERO) -> str:
    return (
        f'<g transform="{hero}">'
        f'<path d="{BUBBLE}" fill="#0d3a85" opacity="0.35" filter="url(#soft)" transform="translate(10,22)"/>'
        f'<path d="{BUBBLE}" fill="url(#bubble)"/>'
        f'<path d="{BUBBLE}" fill="none" stroke="#ffffff" stroke-width="4"/>'
        f'{llama("url(#llama)")}</g>'
    )


def wordmark() -> str:
    word = SHAPES["wordmark"]
    x = 512 - word["width"] * WORD_SCALE / 2
    return (
        f'<path d="{word["d"]}" transform="translate({x:.1f},{BAND_Y + 170}) scale({WORD_SCALE})" '
        'fill="#1e2329"/>'
    )


LAYERS = {"background": background, "database": database, "bubble": bubble, "wordmark": wordmark}


def framed(small: bool = False) -> str:
    """The whole icon in the rounded square of Apple's pre-26 template.

    824 of 1024 with a soft drop shadow: what macOS before 26 expects, and a
    shape Windows and Linux show as it is. `small` leaves out the band and the
    wordmark and draws the database and bubble larger, for the sizes where
    the name could not be read.
    """
    if small:
        art = background(band=False) + database(HERO_SMALL) + bubble(HERO_SMALL)
    else:
        art = "".join(layer() for layer in LAYERS.values())
    frame = 'x="100" y="100" width="824" height="824" rx="185"'
    return svg(
        f'<rect {frame} fill="#000" opacity="0.3" filter="url(#drop)" transform="translate(0,10)"/>'
        f'<clipPath id="frame"><rect {frame}/></clipPath>'
        f'<g clip-path="url(#frame)"><g transform="translate(100,100) scale({824 / 1024})">{art}</g></g>',
        defs=DEFS,
    )


# --- the Icon Composer icon --------------------------------------------------


def layer(name: str, glass: bool) -> dict:
    return {"image-name": f"{name}.png", "name": name, "glass": glass}


def group(layers: list, shadow: bool) -> dict:
    return {
        "layers": layers,
        "shadow": {"kind": "neutral", "opacity": 0.5} if shadow else {"kind": "none", "opacity": 0},
        "translucency": {"enabled": False, "value": 0},
    }


# Top group first. The wordmark stays flat and opaque so it reads; the bubble
# and the database take the glass edge and cast the system's shadow.
ICON_JSON = {
    "groups": [
        group([layer("wordmark", False)], shadow=False),
        group([layer("bubble", True), layer("database", True)], shadow=True),
        group([layer("background", False)], shadow=False),
    ],
    "supported-platforms": {"squares": ["macOS"]},
}


# --- the tray ----------------------------------------------------------------

TRAY = 44  # px: 22 pt at 2x, as the menu bar asks
TRAY_GLYPH = 38  # the bubble's height in the tray image


def tray(loaded: bool) -> str:
    # The bubble with its tail spans 476..900 by 150..636 on the canvas.
    k = TRAY_GLYPH / 486
    fit = f"translate({TRAY / 2 - 688 * k:.3f},{(TRAY - TRAY_GLYPH) / 2 - 150 * k:.3f}) scale({k:.5f})"
    glyph = (
        f'<mask id="llama"><g transform="{fit}"><path d="{BUBBLE}" fill="#fff"/>{llama("#000")}</g></mask>'
        f'<rect width="{TRAY}" height="{TRAY}" fill="#000" mask="url(#llama)"/>'
    )
    if not loaded:
        return svg(glyph, TRAY, defs="")
    # The dot sits top right with a ring of clear space cut around it.
    cut = (
        f'<mask id="cut"><rect width="{TRAY}" height="{TRAY}" fill="#fff"/>'
        '<circle cx="37" cy="7" r="9.5" fill="#000"/></mask>'
    )
    return svg(f'{cut}<g mask="url(#cut)">{glyph}</g><circle cx="37" cy="7" r="6" fill="#000"/>', TRAY, defs="")


# --- writing it all ----------------------------------------------------------


def render(source: str, size: int, work: pathlib.Path) -> bytes:
    """One PNG of an SVG, through tauri icon's resvg."""
    src = work / "source.svg"
    src.write_text(source)
    out = work / "png"
    run("npx", "--no-install", "tauri", "icon", str(src), "--png", str(size), "--output", str(out))
    return (out / f"{size}x{size}.png").read_bytes()


def compile_car(bundle: pathlib.Path, work: pathlib.Path) -> None:
    """Compile the Icon Composer icon the way the Tauri bundler would.

    The app icon is named `Icon`, which the bundler reads back out of the
    catalog for the bundle's `CFBundleIconName`.
    """
    out = work / "car"
    out.mkdir()
    run(
        "xcrun", "actool", str(bundle), "--compile", str(out),
        "--output-format", "human-readable-text", "--notices", "--warnings",
        "--output-partial-info-plist", str(out / "info.plist"),
        "--app-icon", "Icon", "--include-all-app-icons",
        "--enable-on-demand-resources", "NO", "--development-region", "en",
        "--target-device", "mac", "--minimum-deployment-target", "26.0",
        "--platform", "macosx",
    )
    keep_unless_changed(ICONS / "Assets.car", out / "Assets.car", car_reading)


# actool never writes the same bytes twice: a catalog carries a timestamp and
# a fresh UUID in each flattened rendition's name. An icns is compared by its
# images, in any order. So a file is replaced only when what it holds changed,
# and a run with no shape changed leaves the tree clean.


def keep_unless_changed(dest: pathlib.Path, new: pathlib.Path, reading) -> None:
    if dest.exists() and reading(dest) == reading(new):
        return
    dest.write_bytes(new.read_bytes())


def car_reading(path: pathlib.Path) -> list:
    """A catalog's contents, less the stamps actool varies run to run.

    A flattened rendition's digest covers its UUID name, so it goes too; the
    layers it is flattened from keep theirs, so a changed pixel still shows.
    """
    info = json.loads(subprocess.run(
        ["xcrun", "assetutil", "--info", str(path)], check=True, capture_output=True, text=True
    ).stdout)
    for entry in info:
        entry.pop("Timestamp", None)
        name = entry.pop("RenditionName", "")
        if re.search(r"[0-9A-F]{8}-[0-9A-F]{4}-", name):
            entry.pop("SHA1Digest", None)
    return info


def icns_reading(path: pathlib.Path) -> list:
    """An icns's images, in a fixed order."""
    return sorted(icns_chunks(path))


def icns_chunks(path: pathlib.Path) -> list:
    data = path.read_bytes()
    chunks, at = [], 8
    while at < len(data):
        _, size = struct.unpack(">4sI", data[at:at + 8])
        chunks.append(data[at:at + size])
        at += size
    return chunks


# --- the small sizes, without the wordmark -----------------------------------

def icns_from(small: dict, work: pathlib.Path) -> pathlib.Path:
    """The icns, written by iconutil: the small drawing at 16 and 32 px, the
    whole icon from 64 up.

    iconutil and not tauri icon's, because only Apple's encoder writes the 16
    and 32 px slots (ic04, ic05) in the form macOS reads.
    """
    full = {px: render(framed(), px, work) for px in (64, 128, 256, 512, 1024)}
    iconset = work / "gglib.iconset"
    iconset.mkdir()
    for name, png in {
        "icon_16x16.png": small[16], "icon_16x16@2x.png": small[32],
        "icon_32x32.png": small[32], "icon_32x32@2x.png": full[64],
        "icon_128x128.png": full[128], "icon_128x128@2x.png": full[256],
        "icon_256x256.png": full[256], "icon_256x256@2x.png": full[512],
        "icon_512x512.png": full[512], "icon_512x512@2x.png": full[1024],
    }.items():
        (iconset / name).write_bytes(png)
    out = work / "gglib.icns"
    run("iconutil", "--convert", "icns", str(iconset), "--output", str(out))
    return out


def ico_with_small(path: pathlib.Path, small: dict) -> None:
    """The ico with each entry the small drawing has a size for replaced by it."""
    data = path.read_bytes()
    count = struct.unpack("<HHH", data[:6])[2]
    images = []
    for i in range(count):
        w, h, colors, res, planes, bpp, size, off = struct.unpack("<BBBBHHII", data[6 + 16 * i:22 + 16 * i])
        images.append((w, h, colors, res, planes, bpp, small.get(w or 256, data[off:off + size])))
    table, body = b"", b""
    first = 6 + 16 * len(images)
    for w, h, colors, res, planes, bpp, png in images:
        table += struct.pack("<BBBBHHII", w, h, colors, res, planes, bpp, len(png), first + len(body))
        body += png
    path.write_bytes(struct.pack("<HHH", 0, 1, len(images)) + table + body)


def run(*cmd: str) -> None:
    subprocess.run(cmd, cwd=ROOT, check=True, stdout=subprocess.DEVNULL)


def main() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        work = pathlib.Path(tmp)

        bundle = work / "Icon.icon"
        (bundle / "Assets").mkdir(parents=True)
        for name, draw in LAYERS.items():
            (bundle / "Assets" / f"{name}.png").write_bytes(render(svg(draw()), 1024, work))
        (bundle / "icon.json").write_text(json.dumps(ICON_JSON, indent=2) + "\n")
        compile_car(bundle, work)

        (ICONS / "tray-idle.png").write_bytes(render(tray(loaded=False), TRAY, work))
        (ICONS / "tray-active.png").write_bytes(render(tray(loaded=True), TRAY, work))

        master = work / "gglib.svg"
        master.write_text(framed())
        icns = ICONS / "icon.icns"
        before = icns.read_bytes() if icns.exists() else None
        run("npx", "--no-install", "tauri", "icon", str(master), "--output", str(ICONS))
        # tauri icon's icns is replaced by iconutil's below; put the committed
        # one back so it is kept when nothing in it changed.
        if before is None:
            icns.unlink()
        else:
            icns.write_bytes(before)

        small = {px: render(framed(small=True), px, work) for px in SMALL}
        (ICONS / "32x32.png").write_bytes(small[32])
        (ICONS / "Square30x30Logo.png").write_bytes(small[30])
        (ICONS / "Square44x44Logo.png").write_bytes(small[44])
        ico_with_small(ICONS / "icon.ico", small)
        keep_unless_changed(icns, icns_from(small, work), icns_reading)


if __name__ == "__main__":
    main()
