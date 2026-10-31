// Usage: node render.mjs <in.cast> [out.gif] [--text out.txt] [--idle 1.2] [--hold 4] [--rename from=to]...
//
// Plays an asciicast (record.py writes them) into xterm.js in headless
// Chrome, screenshots each change, and has ffmpeg make the GIF. A pause
// longer than --idle seconds is shortened to it, so a download or a model
// load reads as a beat rather than dead air; --hold keeps the last frame up
// before the loop starts again. --rename replaces text in what was printed
// before it is drawn, for a path that should not name this machine's user.
// --text writes what the main terminal shows when the playback ends, as
// plain text with the renames applied, for a code block in the docs; the
// prompt waiting at the end is left off. With no GIF path, that is all it
// does.
//
// A cast with `panes` in its header (record.py's side panes) gets one more
// terminal per pane under the main one, labelled with the pane's name.
//
// Markers in the cast steer the playback: "chapter:<title>" shows a title
// card and keeps the title in the window's title bar, "title:<text>" changes
// the title bar alone, and "speed:<n>" plays
// what follows n times faster, with a badge saying so, until "speed:1".
// FPS (10), FONT_SIZE (15) and CARD (seconds a title card shows, 1.6) are
// read from the environment.
import puppeteer from 'puppeteer-core';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';

const args = process.argv.slice(2);
const [castFile, out] = args.filter((a, i) => !a.startsWith('--') && !args[i - 1]?.startsWith('--'));
const textOut = args.includes('--text') ? args[args.indexOf('--text') + 1] : undefined;
if (!castFile || (!out && !textOut)) {
  console.error('usage: node render.mjs <in.cast> [out.gif] [--text out.txt] [--idle s] [--hold s] [--rename from=to]...');
  process.exit(2);
}
const flag = (name, dflt) => {
  const i = args.indexOf(name);
  return i < 0 ? dflt : +args[i + 1];
};
const idle = flag('--idle', 1.2);
const hold = flag('--hold', 4);
const renames = args.flatMap((a, i) => {
  if (a !== '--rename') return [];
  const pair = args[i + 1];
  if (!(pair?.indexOf('=') > 0)) throw new Error(`--rename wants from=to, not ${pair}`);
  return [[pair.slice(0, pair.indexOf('=')), pair.slice(pair.indexOf('=') + 1)]];
});
const fps = +(process.env.FPS || 10);
const card = +(process.env.CARD || 1.6);

const [header, ...lines] = fs.readFileSync(castFile, 'utf8').trim().split('\n');
const { width: cols, height: rows, panes = [] } = JSON.parse(header);

// The playback clock: real gaps divided by the speed in force, each capped
// at --idle, plus the time the title cards hold the screen. What prints in
// one frame is folded into one write. A write that ends partway into a
// --rename's text keeps that tail back for the next one, so a rename still
// matches when the pty split it across two reads.
const actions = [];
let clock = 0;
let prev = 0;
let speed = 1;
// Per stream: the main terminal is 'o', a pane is 'o@<name>'.
const held = {};
// The longest end of `text` that a rename's text starts with.
const heldTail = (text) => {
  let longest = '';
  for (const [from] of renames) {
    for (let n = Math.min(from.length - 1, text.length); n > longest.length; n--) {
      if (text.endsWith(from.slice(0, n))) {
        longest = text.slice(-n);
        break;
      }
    }
  }
  return longest;
};
for (const line of lines) {
  let [t, kind, data] = JSON.parse(line);
  clock += Math.min((t - prev) / speed, idle);
  prev = t;
  if (kind === 'm') {
    const [what, value] = [data.slice(0, data.indexOf(':')), data.slice(data.indexOf(':') + 1)];
    if (what === 'chapter') {
      actions.push({ at: Math.floor(clock * fps) / fps, chapter: value });
      clock += card;
    } else if (what === 'title') {
      actions.push({ at: Math.floor(clock * fps) / fps, title: value });
    } else if (what === 'speed') {
      speed = +value;
      if (!(speed > 0)) throw new Error(`speed must be a positive number, not ${value}`);
      actions.push({ at: Math.floor(clock * fps) / fps, badge: speed > 1 ? `▶▶ ${speed}×` : '' });
    }
    continue;
  }
  if (!kind.startsWith('o')) continue;
  const at = Math.floor(clock * fps) / fps;
  const text = (held[kind] ?? '') + data;
  held[kind] = heldTail(text);
  data = text.slice(0, text.length - held[kind].length);
  const last = actions.findLast((a) => a.stream === kind);
  if (last && last.at === at) last.text += data;
  else actions.push({ at, stream: kind, text: data });
}
for (const [stream, text] of Object.entries(held)) {
  if (text) actions.push({ at: actions.at(-1)?.at ?? 0, stream, text });
}
for (const a of actions) {
  if (a.text !== undefined) for (const [from, to] of renames) a.text = a.text.replaceAll(from, to);
}

const require = createRequire(import.meta.url);
const xtermDir = path.dirname(require.resolve('@xterm/xterm/package.json'));

const browser = await puppeteer.launch({
  executablePath: process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  headless: true,
  args: ['--force-color-profile=srgb'],
});
const page = await browser.newPage();
await page.setViewport({ width: 1600, height: 1200, deviceScaleFactor: 1 });
await page.setContent(`<!doctype html><html><body style="margin:0;background:#0b0b0d;font-family:-apple-system,'Helvetica Neue',sans-serif">
  <div id="win" style="display:inline-block;position:relative;background:#0b0b0d;padding:18px 28px 28px">
    <div style="position:relative;display:flex;align-items:center;gap:9px;height:20px;margin:0 0 20px 4px">
      <span style="width:13px;height:13px;border-radius:50%;background:#ff5f57"></span>
      <span style="width:13px;height:13px;border-radius:50%;background:#febc2e"></span>
      <span style="width:13px;height:13px;border-radius:50%;background:#28c840"></span>
      <span id="title" style="position:absolute;left:0;right:0;text-align:center;color:#8a8a90;font-size:14px;pointer-events:none"></span>
      <span id="badge" style="margin-left:auto;color:#f2c55c;font-size:14px;font-weight:600;letter-spacing:.5px"></span>
    </div>
    <div id="term"></div>
    ${panes.map((p) => `<div style="color:#8a8a90;font-size:13px;letter-spacing:.5px;margin:14px 0 8px 2px;padding-top:12px;border-top:1px solid #26262b">${p.name}</div><div id="pane-${p.name}"></div>`).join('')}
    <div id="card" style="display:none;position:absolute;inset:58px 0 0 0;background:rgba(11,11,13,.9);align-items:center;justify-content:center;flex-direction:column;gap:10px">
      <div id="cardstep" style="color:#5fd38d;font-size:15px;font-weight:600;letter-spacing:2px"></div>
      <div id="cardtext" style="color:#f2f2f2;font-size:34px;font-weight:600"></div>
    </div>
  </div></body></html>`);
await page.addStyleTag({ path: path.join(xtermDir, 'css/xterm.css') });
await page.addScriptTag({ path: path.join(xtermDir, 'lib/xterm.js') });
// No scrollbar: a picture of a terminal has nothing to scroll.
await page.addStyleTag({ content: '.xterm .scrollbar, .xterm .xterm-scrollable-element > .scrollbar { display: none !important; } .xterm-viewport { overflow: hidden !important; }' });
await page.evaluate(
  async (cols, rows, panes, fontSize) => {
    await document.fonts.load(`${fontSize}px Menlo`);
    const make = (rows, el) => {
      const t = new Terminal({
        cols,
        rows,
        fontSize,
        fontFamily: '"SF Mono", Menlo, monospace',
        lineHeight: 1.15,
        cursorBlink: false,
        allowProposedApi: true,
        theme: {
          background: '#0b0b0d', foreground: '#e6e6e6', cursor: '#e6e6e6',
          black: '#1c1c1f', red: '#ff6b6b', green: '#5fd38d', yellow: '#f2c55c',
          blue: '#6cb6ff', magenta: '#c792ea', cyan: '#5ccfe6', white: '#d0d0d0',
          brightBlack: '#7a7a80', brightRed: '#ff8787', brightGreen: '#7ee2a8',
          brightYellow: '#ffd88a', brightBlue: '#91caff', brightMagenta: '#dcb0ff',
          brightCyan: '#86e1f0', brightWhite: '#ffffff',
        },
      });
      t.open(document.getElementById(el));
      return t;
    };
    window.terms = { o: make(rows, 'term') };
    for (const p of panes) terms[`o@${p.name}`] = make(p.rows, `pane-${p.name}`);
  },
  cols,
  rows,
  panes,
  +(process.env.FONT_SIZE || 15),
);
const win = await page.$('#win');

const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'gglib-render-'));
const frames = [];
let last = '';
const shoot = async (at) => {
  if (!out) return;
  const png = await win.screenshot({ type: 'png' });
  const hash = createHash('sha1').update(png).digest('hex');
  if (hash === last) return;
  last = hash;
  const file = path.join(dir, `f${String(frames.length).padStart(5, '0')}.png`);
  fs.writeFileSync(file, png);
  frames.push({ file, at });
};

await shoot(0);
let chapters = 0;
for (const [i, a] of actions.entries()) {
  if (a.text !== undefined) {
    await page.evaluate((s, t) => new Promise((r) => terms[s].write(t, r)), a.stream, a.text);
    // Two streams that print in one frame make one frame.
    if (actions[i + 1]?.text !== undefined && actions[i + 1].at === a.at) continue;
  } else if (a.badge !== undefined) {
    await page.evaluate((b) => (document.getElementById('badge').textContent = b), a.badge);
  } else if (a.title !== undefined) {
    await page.evaluate((t) => (document.getElementById('title').textContent = t), a.title);
  } else {
    chapters += 1;
    await page.evaluate((title, n) => {
      document.getElementById('title').textContent = title;
      document.getElementById('cardstep').textContent = String(n);
      document.getElementById('cardtext').textContent = title;
      document.getElementById('card').style.display = 'flex';
    }, a.chapter, chapters);
    await shoot(a.at);
    await page.evaluate(() => (document.getElementById('card').style.display = 'none'));
    await shoot(a.at + card);
    continue;
  }
  await shoot(a.at);
}
if (textOut) {
  const shown = await page.evaluate(() => {
    const buf = terms.o.buffer.active;
    // A line the terminal wrapped is one line of output.
    const lines = [];
    for (let i = 0; i < buf.length; i++) {
      const line = buf.getLine(i);
      if (!line) continue;
      if (line.isWrapped && lines.length) lines[lines.length - 1] += line.translateToString(true);
      else lines.push(line.translateToString(true));
    }
    return lines;
  });
  while (shown.length && /^(\$ ?)?$/.test(shown.at(-1))) shown.pop();
  fs.writeFileSync(textOut, `${shown.join('\n')}\n`);
  console.log(`wrote ${textOut}: ${shown.length} lines`);
}
const end = (frames.at(-1)?.at ?? 0) + hold;
await browser.close();
if (!out) {
  fs.rmSync(dir, { recursive: true });
  process.exit(0);
}

// The concat demuxer takes each frame's duration; the last is named twice so
// its duration is honoured.
const list = frames.map((f, i) => `file '${f.file}'\nduration ${Math.max(0.02, (frames[i + 1]?.at ?? end) - f.at).toFixed(3)}`);
list.push(`file '${frames.at(-1).file}'`);
fs.writeFileSync(path.join(dir, 'list.txt'), list.join('\n'));
execFileSync('ffmpeg', [
  '-v', 'error', '-y', '-f', 'concat', '-safe', '0', '-i', path.join(dir, 'list.txt'),
  '-vf', 'split[a][b];[a]palettegen=max_colors=128:stats_mode=full[p];[b][p]paletteuse=dither=none',
  '-fps_mode', 'vfr', out,
]);
fs.rmSync(dir, { recursive: true });
console.log(`wrote ${out}: ${frames.length} frames, ${end.toFixed(1)}s`);
