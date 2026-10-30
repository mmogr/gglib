// Usage: node shot.mjs <url> <out.png> [steps.json]
//
// Opens the web dashboard in headless Chrome and screenshots it after the
// steps have run. W and H set the viewport (1440×900 by default), always at
// 2× so the picture is crisp at README width; SCHEME=light asks for the
// light theme; DUMP=<file> also writes the page's visible text, which is the
// quickest way to learn what a step should click.
import puppeteer from 'puppeteer-core';
import fs from 'node:fs';

const [url, out, stepsFile] = process.argv.slice(2);
if (!url || !out) {
  console.error('usage: node shot.mjs <url> <out.png> [steps.json]');
  process.exit(2);
}
const steps = stepsFile ? JSON.parse(fs.readFileSync(stepsFile, 'utf8')) : [];

const browser = await puppeteer.launch({
  executablePath: process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  headless: true,
  args: ['--hide-scrollbars', '--force-color-profile=srgb'],
});
const page = await browser.newPage();
await page.setViewport({ width: +(process.env.W || 1440), height: +(process.env.H || 900), deviceScaleFactor: 2 });
await page.emulateMediaFeatures([{ name: 'prefers-color-scheme', value: process.env.SCHEME || 'dark' }]);
page.on('pageerror', (e) => console.error('pageerror', e.message));

await page.goto(url, { waitUntil: 'load', timeout: 30000 });
// The dashboard holds an event stream open, so Chrome's network-idle never
// comes; wait for the page to have rendered something instead.
await page.waitForFunction(() => document.body.innerText.length > 200, { timeout: 30000 });
await new Promise((r) => setTimeout(r, 1500));

for (const s of steps) {
  if (s.click) {
    const el = await page.waitForSelector(s.click, { timeout: 10000 });
    await el.click();
  }
  if (s.clickText) {
    // The deepest element whose whole text is this, so a row's label wins
    // over the row and the row over the list.
    const handle = await page.waitForFunction((t) => {
      const all = [...document.querySelectorAll('button, a, [role=tab], [role=button], [role=menuitem], li, span, div, p, h1, h2, h3')];
      const hits = all.filter((e) => e.textContent.trim() === t);
      return hits.length ? hits[hits.length - 1] : null;
    }, { timeout: 10000 }, s.clickText);
    await handle.asElement().click();
  }
  if (s.type) await page.keyboard.type(s.type, { delay: 10 });
  if (s.press) await page.keyboard.press(s.press);
  if (s.waitText) await page.waitForFunction((t) => document.body.innerText.includes(t), { timeout: s.timeout || 60000 }, s.waitText);
  if (s.sleep) await new Promise((r) => setTimeout(r, s.sleep));
}
await new Promise((r) => setTimeout(r, 600));
await page.screenshot({ path: out, type: 'png' });
if (process.env.DUMP) fs.writeFileSync(process.env.DUMP, await page.evaluate(() => document.body.innerText));
await browser.close();
console.log('wrote', out);
