// One pass through the web app in a real browser: every screen, after hydration, with no console errors.
// `step` lines are prerequisites and stop the run; `check` lines record a failure and carry on, so one run shows them all.
import { chromium } from "playwright";
import assert from "node:assert/strict";
import { execSync } from "node:child_process";

const base = process.env.E2E_URL ?? "http://127.0.0.1:8099";
const user = process.env.E2E_USER, password = process.env.E2E_PASSWORD;
const longPrompt = "slow ambient pads over distant rain ".repeat(13).trim().slice(0, 480);

const browser = await chromium.launch();
const page = await browser.newPage();
const problems = [];
const failed = [];
page.on("console", (m) => m.type() === "error" && problems.push(`console: ${m.text()}`));
page.on("pageerror", (e) => problems.push(`page: ${e.message}`));
const step = (name) => console.log(`- ${name}`);
async function check(name, fn) {
  try {
    await fn();
    console.log(`  ok: ${name}`);
  } catch (e) {
    console.log(`  FAILED: ${name}: ${e.message.split("\n")[0]}`);
    const text = await page.evaluate(() => document.body.innerText.replace(/\s+/g, " ").slice(0, 300)).catch(() => "");
    console.log(`    at ${page.url()}: ${text}`);
    failed.push(name);
  }
}

async function noSidewaysScroll(path) {
  await page.setViewportSize({ width: 360, height: 800 });
  await page.goto(base + path);
  await page.waitForLoadState("networkidle");
  const w = await page.evaluate(() => document.documentElement.scrollWidth);
  assert.ok(w <= 360, `${path} is ${w}px wide at 360`);
  await page.setViewportSize({ width: 1280, height: 900 });
}

// WCAG contrast ratio of two computed rgb() colours.
function contrast(a, b) {
  const lum = (c) => {
    const [r, g, bl] = c.match(/\d+(\.\d+)?/g).slice(0, 3).map((v) => { v = v / 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; });
    return 0.2126 * r + 0.7152 * g + 0.0722 * bl;
  };
  const [x, y] = [lum(a), lum(b)].sort((m, n) => n - m);
  return (x + 0.05) / (y + 0.05);
}

async function tallEnough(locator, what) {
  const box = await locator.boundingBox();
  assert.ok(box && box.height >= 24, `${what} is ${box?.height}px tall, under 24`);
}

// Where a hovered control's caption lands, left and right, in page pixels.
async function captionEdges(locator) {
  await locator.hover();
  return locator.evaluate((el) => {
    const a = getComputedStyle(el, "::after"), r = el.getBoundingClientRect();
    const left = r.left + parseFloat(a.left) + new DOMMatrix(a.transform === "none" ? undefined : a.transform).m41;
    return { left, right: left + parseFloat(a.width) + parseFloat(a.paddingLeft) + parseFloat(a.paddingRight), width: innerWidth };
  });
}

async function makeSong(prompt, seconds) {
  await page.getByLabel("What it sounds like").fill(prompt);
  await page.getByLabel("Length").selectOption(String(seconds));
  await page.getByRole("button", { name: "Make the song" }).click();
}

await check("asset names carry a hash and are cached for good", async () => {
  const html = await (await page.request.get(base + "/sign-in")).text();
  const css = html.match(/\/pkg\/maio\.[A-Za-z0-9_-]+\.css/);
  assert.ok(css, "a hashed stylesheet link");
  assert.match(html, /\/pkg\/maio\.[A-Za-z0-9_-]+\.js/, "a hashed script");
  const res = await page.request.get(base + css[0]);
  assert.equal(res.status(), 200);
  assert.match(res.headers()["cache-control"] ?? "", /immutable/);
});

await check("the e2e config is committed, not ignored", async () => {
  let ignored = true;
  try { execSync("git check-ignore -q e2e/maio.toml", { cwd: ".." }); } catch { ignored = false; }
  assert.equal(ignored, false, "e2e/maio.toml is git-ignored");
});

step("strangers land on sign-in");
await page.goto(base + "/");
assert.equal(new URL(page.url()).pathname, "/sign-in");
await check("the Keep me signed in box is big enough to hit", async () => {
  await tallEnough(page.getByRole("checkbox"), "the Keep me signed in box");
});

await check("a failed sign-in says so and puts focus back in the password field", async () => {
  await page.waitForLoadState("networkidle");
  await page.getByLabel("Username").fill(user);
  await page.getByLabel("Password").fill("not-the-password");
  await page.getByRole("button", { name: "Sign in" }).click();
  await page.getByRole("alert").filter({ hasText: /./ }).waitFor({ timeout: 5_000 });
  assert.equal(await page.evaluate(() => document.activeElement?.getAttribute("type")), "password");
});

step("sign in, typing before the app has loaded");
await check("typing before the app loads survives hydration", async () => {
  await page.route("**/*.wasm", (r) => setTimeout(() => r.continue().catch(() => {}), 1500));
  await page.goto(base + "/sign-in");
  await page.getByLabel("Username").fill(user);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Sign in" }).click({ timeout: 10_000 });
  await page.waitForURL(base + "/", { timeout: 5_000 });
  await page.unroute("**/*.wasm");
});
if (new URL(page.url()).pathname !== "/") {
  await page.goto(base + "/sign-in");
  await page.waitForLoadState("networkidle");
  await page.getByLabel("Username").fill(user);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Sign in" }).click();
  await page.waitForURL(base + "/");
}
await page.getByRole("heading", { name: "Make a song" }).waitFor();

await check("a taller prompt box leaves the Make button its size", async () => {
  await page.getByLabel("What it sounds like").evaluate((t) => (t.style.height = "300px"));
  const h = await page.getByRole("button", { name: "Make the song" }).evaluate((b) => b.getBoundingClientRect().height);
  assert.equal(Math.round(h), 84);
  await page.getByLabel("What it sounds like").evaluate((t) => (t.style.height = ""));
});

step("make a song with a long prompt");
await page.locator(".engine.ready").waitFor({ timeout: 60_000 });

await check("the engine is an icon with a status dot: its state is in its caption and its name, not on screen", async () => {
  const button = page.locator(".engine");
  assert.equal((await button.innerText()).trim(), "", "no words in the header");
  assert.equal(await button.locator("svg").count(), 1, "an icon");
  assert.ok(await button.locator("i").count(), "a status dot");
  assert.match(await button.getAttribute("data-tip"), /ACE-Step 1\.5/, "the caption says which engine and how it is");
  assert.match(await button.getAttribute("aria-label"), /ACE-Step 1\.5.*show the engines/);
  const box = await button.boundingBox();
  assert.ok(box.width >= 44 && box.height >= 44, `${box.width}×${box.height}`);
});

await check("the engine line opens a panel that says what each engine is doing", async () => {
  await page.locator(".engine").click();
  const panel = page.getByRole("region", { name: "Engines" });
  await panel.waitFor();
  const text = await panel.innerText();
  assert.match(text, /ACE-Step 1\.5/);
  assert.match(text, /Ready to make songs\./);
  assert.match(text, /Model acestep-v15-turbo/);
  await page.keyboard.press("Escape");
  await panel.waitFor({ state: "hidden" });
});

await check("every job's bar sits in the same place, whatever its text says", async () => {
  // One slot: the first is making ("about … min"), the second waiting.
  await makeSong("slow one", 600);
  await page.locator(".job").first().getByText(/about/).waitFor();
  await makeSong("slow two", 600);
  await page.locator(".job").nth(1).getByText("waiting").waitFor();
  const bars = await page.locator(".job .bar").evaluateAll((els) => els.map((e) => { const r = e.getBoundingClientRect(); return [Math.round(r.left), Math.round(r.width)]; }));
  assert.equal(bars.length, 2);
  assert.deepEqual(bars[0], bars[1], `bars at ${JSON.stringify(bars)}`);
  for (const name of ["Cancel slow one", "Cancel slow two"]) await page.getByRole("button", { name }).click();
  await page.getByRole("heading", { name: "Being made" }).waitFor({ state: "hidden" });
});

await makeSong(longPrompt, 30);
await page.getByRole("heading", { name: "Being made" }).waitFor();
const firstSong = page.locator(".song").first();
await firstSong.waitFor({ timeout: 120_000 });
const title = (await firstSong.locator(".song-title").textContent()).trim();

await check("without MediaMTX set up the header has no MediaMTX line", async () => {
  const status = await (await page.request.get(base + "/api/v1/status")).json();
  assert.equal(status.relay, null);
  assert.equal(await page.locator("button.relay").count(), 0);
});

await check("the header's status changes when the server says so, never on a timer", async () => {
  let fetched = 0;
  const onRequest = (r) => { if (r.url().endsWith("/api/v1/status")) fetched++; };
  page.on("request", onRequest);
  await page.waitForTimeout(31_000);
  page.off("request", onRequest);
  assert.equal(fetched, 0, "nothing changed, so nothing was fetched");
});

await check("a page listens for events before it loads anything, so no change falls in between", async () => {
  const order = [];
  const onRequest = (r) => order.push(new URL(r.url()).pathname);
  page.on("request", onRequest);
  await page.reload();
  await page.locator(".song").first().waitFor();
  page.off("request", onRequest);
  const listening = order.indexOf("/api/v1/events");
  const loads = order.map((p, i) => [p, i]).filter(([p]) => /^\/api\/v1\/(songs|playlists|status)/.test(p));
  assert.ok(listening >= 0 && loads.length > 0, order.join(", "));
  assert.ok(loads.every(([, i]) => i > listening), order.join(", "));
});

await check("a song's title opens its details, with downloads and Make one like this", async () => {
  await page.getByRole("button", { name: `Details for ${title}` }).first().click();
  const dialog = page.getByRole("dialog");
  await dialog.getByText("Described as").waitFor();
  const [download] = await Promise.all([page.waitForEvent("download"), dialog.getByRole("link", { name: "Download as MP3" }).click()]);
  assert.match(download.suggestedFilename(), /\.mp3$/);
  for (const chip of await dialog.locator(".downloads .chip").all()) {
    await chip.hover();
    const tip = await chip.evaluate((el) => {
      const s = getComputedStyle(el, "::after"), r = el.getBoundingClientRect(), box = el.closest("dialog, .thaw-dialog-content, .thaw-dialog-body").getBoundingClientRect();
      const left = r.left + parseFloat(s.left), top = r.bottom - parseFloat(s.bottom) - parseFloat(s.height);
      return { text: s.content, inside: left >= box.left && left + parseFloat(s.width) <= box.right && top >= box.top };
    });
    assert.match(tip.text, /Download as /, "a caption on each download");
    assert.ok(tip.inside, `the caption ${tip.text} runs out of the dialog`);
  }
  await dialog.getByRole("button", { name: "Make one like this" }).click();
  await dialog.waitFor({ state: "hidden" });
  assert.equal(await page.getByLabel("What it sounds like").inputValue(), longPrompt);
  await page.locator(".chip.seed").waitFor();
  await page.getByRole("button", { name: "Remove the seed: make a new take" }).click();
  assert.equal(await page.locator(".chip.seed").count(), 0);
  await page.getByLabel("What it sounds like").fill("");
});

await check("a double click on Make starts one song", async () => {
  const posts = [];
  const count = (r) => r.method() === "POST" && r.url().endsWith("/api/v1/songs") && posts.push(r);
  page.on("request", count);
  await page.getByLabel("What it sounds like").fill("double click test");
  await page.getByLabel("Length").selectOption("30");
  await page.getByRole("button", { name: "Make the song" }).dblclick();
  await page.waitForTimeout(1500);
  page.off("request", count);
  assert.equal(posts.length, 1, `${posts.length} songs asked for`);
});
await page.locator(".song", { hasText: "double click test" }).first().waitFor({ timeout: 120_000 });

await check("Cancel keeps keyboard focus while a song is being made", async () => {
  await makeSong("a long one to cancel", 600);
  const cancel = page.getByRole("button", { name: /^Cancel a long one to cancel/ });
  await cancel.waitFor();
  await cancel.focus();
  await page.waitForTimeout(5000); // a couple of progress updates
  const focused = await page.evaluate(() => document.activeElement?.getAttribute("aria-label") ?? "");
  assert.match(focused, /^Cancel a long one/, `focus is on "${focused}"`);
  await page.keyboard.press("Enter");
  await cancel.waitFor({ state: "detached", timeout: 10_000 });
});

step("play it");
await page.getByRole("button", { name: `Play ${title}` }).first().click();
await page.locator(".now-title", { hasText: title }).waitFor();

await check("the playlist marks are an image screen readers name, and small controls are big enough to hit", async () => {
  assert.equal(await page.locator(".song .marks").first().getAttribute("role"), "img");
  await tallEnough(page.getByRole("slider", { name: "Seek" }), "the seek bar");
  await tallEnough(page.getByRole("slider", { name: "Volume" }), "the volume slider");
  await tallEnough(page.locator(".song-title").first(), "a song title");
  await tallEnough(page.getByRole("searchbox"), "the search box");
  await tallEnough(page.getByLabel("Mood"), "the Mood select");
});

await check("keyboard focus shows on the mustard player bar", async () => {
  for (let i = 0; i < 80; i++) {
    await page.keyboard.press("Tab");
    if (await page.evaluate(() => !!document.activeElement?.closest(".player"))) break;
  }
  const { outline, style, bg } = await page.evaluate(() => {
    const s = getComputedStyle(document.activeElement);
    return { outline: s.outlineColor, style: s.outlineStyle, bg: getComputedStyle(document.querySelector(".player")).backgroundColor };
  });
  assert.notEqual(style, "none", "no outline");
  assert.notEqual(outline, bg, `outline ${outline} on background ${bg}`);
});

step("make a playlist and add two songs");
await check("the shelf says what its tiles do in words: its scrolling row would cut a caption off", async () => {
  assert.match(await page.locator(".tile.new").innerText(), /New playlist/);
  assert.equal(await page.locator(".shelf [data-tip]").count(), 0);
});
await page.getByRole("button", { name: "New playlist" }).click();
await page.getByRole("dialog").getByRole("textbox").fill("Smoke test list");
await page.getByRole("button", { name: "Create" }).click();
await page.waitForURL(/\/playlists\//);
await page.getByRole("heading", { name: "Smoke test list" }).waitFor();
const playlistPath = new URL(page.url()).pathname;
await page.getByRole("link", { name: /Maio/ }).click();
await page.getByRole("button", { name: `Add ${title} to a playlist` }).click();
await page.getByRole("button", { name: "Smoke test list" }).click();
const other = (await page.locator(".song", { hasText: "double click test" }).first().locator(".song-title").textContent()).trim();
await page.getByRole("button", { name: `Add ${other} to a playlist` }).first().click();
await page.getByRole("button", { name: "Smoke test list" }).click();
// A click on the shelf tile: in-app navigation, as a person moves around, so the player keeps playing.
await page.getByRole("link", { name: /Smoke test list/ }).click();
await page.waitForURL(base + playlistPath);
await page.locator(".track").nth(1).waitFor();
assert.equal(await page.locator(".track").count(), 2);

await check("the Playlists tab lists the playlist", async () => {
  await page.getByRole("navigation", { name: "Sections" }).getByRole("link", { name: "Playlists" }).click();
  await page.waitForURL(base + "/playlists");
  const row = page.locator(".playlist-row", { hasText: "Smoke test list" });
  await row.waitFor({ timeout: 5_000 });
  assert.match(await row.innerText(), /\d+ songs?/, "the row says how many songs");
  assert.ok(await row.locator(".strip").count(), "the row starts with its colour, like its banner");
  const look = await row.evaluate((el) => getComputedStyle(el).backgroundColor);
  assert.equal(look, "rgba(0, 0, 0, 0)", "rows like the songs list, not cards");
  await row.getByRole("button", { name: "Play Smoke test list" }).waitFor();
  await page.getByRole("button", { name: "New playlist" }).waitFor();
  await row.getByRole("link", { name: /Smoke test list/ }).click();
  await page.waitForURL(base + playlistPath);
  await page.locator(".track").nth(1).waitFor();
});

await check("a change made elsewhere shows at once, carried by its event, with nothing fetched", async () => {
  const pid = playlistPath.split("/").pop();
  const song = (await (await page.request.get(base + "/api/v1/songs?q=" + encodeURIComponent(title))).json()).songs[0];
  const fetched = [];
  const onRequest = (r) => { if (r.method() === "GET" && /\/api\/v1\/(songs|playlists)/.test(r.url())) fetched.push(r.url()); };
  page.on("request", onRequest);
  await page.request.patch(base + "/api/v1/songs/" + song.id, { headers: { origin: base }, data: { title: "Renamed elsewhere" } });
  await page.locator(".track", { hasText: "Renamed elsewhere" }).waitFor({ timeout: 3_000 });
  await page.request.patch(base + "/api/v1/playlists/" + pid, { headers: { origin: base }, data: { name: "Smoke list elsewhere" } });
  await page.getByRole("heading", { name: "Smoke list elsewhere" }).waitFor({ timeout: 3_000 });
  page.off("request", onRequest);
  assert.deepEqual(fetched, [], "the events carried the new title and name");
  await page.request.patch(base + "/api/v1/songs/" + song.id, { headers: { origin: base }, data: { title } });
  await page.request.patch(base + "/api/v1/playlists/" + pid, { headers: { origin: base }, data: { name: "Smoke test list" } });
  await page.getByRole("heading", { name: "Smoke test list" }).waitFor({ timeout: 3_000 });
});

await check("on a phone the playlist's name fits two lines, its songs keep their titles, and the header has two tidy rows", async () => {
  // Resized in place, not reloaded: the next step checks the player kept its song.
  await page.setViewportSize({ width: 390, height: 844 });
  await page.locator(".track").first().waitFor();
  const lines = await page.locator(".band h1").evaluate((h) => Math.round(h.getBoundingClientRect().height / parseFloat(getComputedStyle(h).lineHeight)));
  assert.ok(lines <= 2, `the name takes ${lines} lines`);
  const title = await page.locator(".track .track-title").first().evaluate((t) => t.getBoundingClientRect().width);
  assert.ok(title >= 150, `a song's title has ${Math.round(title)}px`);
  // The header is two rows on purpose: the wordmark and every icon on the first, the tabs across the full width below.
  const rows = await page.locator("header.top").evaluate((top) => {
    const box = (e) => e.getBoundingClientRect();
    const tabs = top.querySelector(".tabs"), others = [...top.children].filter((e) => e !== tabs && box(e).height > 0);
    return { firstRow: new Set(others.map((e) => Math.round(box(e).top + box(e).height / 2))).size, tabsBelow: box(tabs).top >= Math.max(...others.map((e) => box(e).bottom)), tabsWidth: box(tabs).width, width: box(top).width };
  });
  assert.equal(rows.firstRow, 1, "the wordmark and the icons share one row");
  assert.ok(rows.tabsBelow && rows.tabsWidth >= rows.width - 40, `the tabs sit below across the width (${Math.round(rows.tabsWidth)} of ${Math.round(rows.width)}px)`);
  // The live strip: the switch, the listeners and the round buttons on one row, with no divider left dangling.
  const strip = await page.locator(".live-strip").evaluate((s) => {
    const shown = (e) => e.getBoundingClientRect().height > 0;
    const row = [s.querySelector(".live-switch"), s.querySelector(".listeners"), ...s.querySelectorAll(".chipish")].filter(shown);
    return { tops: row.map((e) => Math.round(e.getBoundingClientRect().top + e.getBoundingClientRect().height / 2)), seps: [...s.querySelectorAll(".sep")].filter(shown).length };
  });
  assert.ok(Math.max(...strip.tops) - Math.min(...strip.tops) <= 12, `the live strip wraps its controls: ${strip.tops.join(", ")}`);
  assert.equal(strip.seps, 0, "no divider on a phone");
  await page.setViewportSize({ width: 1280, height: 900 });
});

await check("Add songs offers only songs not in the playlist, under a search as wide as the dialog", async () => {
  const inList = (await page.locator(".track .track-title").allInnerTexts()).map((t) => t.trim());
  await page.getByRole("button", { name: "Add songs" }).click();
  const dialog = page.getByRole("dialog", { name: "Add songs" });
  await dialog.waitFor();
  // A plain labelled field, like the other dialogs' fields: no pill around it for the focus ring to clash with.
  assert.equal(await dialog.locator(".search").count(), 0, "no search pill");
  const field = dialog.getByRole("textbox", { name: "Search" });
  await field.click();
  const [box, body] = [await field.boundingBox(), await dialog.locator(".pick").boundingBox()];
  assert.ok(Math.abs(box.width - body.width) <= 4, `the field is ${box.width}px wide, the list ${body.width}px`);
  await dialog.locator(".pick-empty, .pick [role=checkbox]").first().waitFor();
  const offered = (await dialog.locator(".pick").innerText()).split("\n").map((t) => t.trim()).filter(Boolean);
  for (const title of inList) assert.ok(!offered.includes(title), `${title} is already in the playlist`);
  // Every song in this library is on the playlist: the dialog says so instead of an empty list.
  assert.deepEqual(offered, ["Every song is already in this playlist."]);
  await dialog.getByRole("button", { name: "Cancel" }).click();
});

step("the player kept its song across pages");
await page.locator(".now-title", { hasText: title }).waitFor();

await check("a song title on a playlist is big enough to hit", async () => {
  await tallEnough(page.locator(".track-title button").first(), "a song title on a playlist");
});

await check("a playlist's play button takes the band's colour", async () => {
  const ok = await page.evaluate(() => {
    const band = document.querySelector(".band");
    const play = band.querySelector(".icon.solid.big");
    if (!band.style.getPropertyValue("--band")) return "the band sets no --band colour";
    return getComputedStyle(play).color === getComputedStyle(band).backgroundColor || `${getComputedStyle(play).color} vs ${getComputedStyle(band).backgroundColor}`;
  });
  assert.equal(ok, true);
});

await check("the playlist's Play turns to Pause while it plays", async () => {
  await page.getByRole("button", { name: "Play Smoke test list" }).click();
  await page.getByRole("button", { name: "Pause Smoke test list" }).click({ timeout: 5_000 });
  await page.getByRole("button", { name: "Play Smoke test list" }).waitFor({ timeout: 5_000 });
});

await check("after the list ends, the player's Play plays again", async () => {
  await page.getByRole("button", { name: "Play Smoke test list" }).click();
  await page.waitForTimeout(500);
  await page.locator("audio").evaluate((a) => (a.currentTime = a.duration - 0.3)); // the last song of a 2-song list? skip to its end
  await page.getByRole("button", { name: "Next song" }).click(); // to the last song
  await page.waitForTimeout(500);
  await page.locator("audio").evaluate((a) => (a.currentTime = a.duration - 0.3));
  await page.locator(".player").getByRole("button", { name: "Play", exact: true }).waitFor({ timeout: 5_000 });
  await page.locator(".player").getByRole("button", { name: "Play", exact: true }).click();
  await page.locator(".player").getByRole("button", { name: "Pause", exact: true }).waitFor({ timeout: 5_000 });
  await page.locator(".player").getByRole("button", { name: "Pause", exact: true }).click();
});

await check("the band's Shuffle play starts the playlist shuffled and is no switch", async () => {
  const b = page.locator(".band").getByRole("button", { name: /^Shuffle play/ });
  assert.equal(await b.getAttribute("aria-pressed"), null, "a button, not a second shuffle switch");
  await b.click();
  await page.locator(".player").getByRole("button", { name: "Pause", exact: true }).waitFor({ timeout: 5_000 });
  const shuffle = page.locator(".player").getByRole("button", { name: /^Shuffle/ });
  assert.equal(await shuffle.getAttribute("aria-pressed"), "true", "the player's shuffle is on");
  await shuffle.click();
  await page.locator(".player").getByRole("button", { name: "Pause", exact: true }).click();
});

await check("arrow keys move a song more than one step", async () => {
  const first = (await page.locator(".track-title").first().textContent()).trim();
  await page.locator(".grip").first().focus();
  await page.keyboard.press("ArrowDown");
  await page.waitForTimeout(500);
  const stillOnGrip = await page.evaluate(() => document.activeElement?.classList.contains("grip"));
  assert.ok(stillOnGrip, "focus left the grip after one move");
  assert.equal((await page.locator(".track-title").nth(1).textContent()).trim(), first);
  await page.keyboard.press("ArrowUp");
  await page.waitForTimeout(500);
  assert.equal((await page.locator(".track-title").first().textContent()).trim(), first);
});

await check("fast arrow presses end in the order on screen", async () => {
  await page.locator(".grip").first().focus();
  for (const key of ["ArrowDown", "ArrowUp", "ArrowDown", "ArrowUp", "ArrowDown"]) await page.keyboard.press(key);
  await page.waitForTimeout(1500);
  const shown = await page.locator(".track-title").allTextContents();
  await page.reload();
  await page.locator(".track").nth(1).waitFor();
  assert.deepEqual(await page.locator(".track-title").allTextContents(), shown);
});

await check("Escape closes a menu and puts focus back on its button", async () => {
  await page.getByRole("button", { name: /^Account/ }).click();
  await page.getByRole("button", { name: "Sign out", exact: true }).waitFor();
  await page.keyboard.press("Escape");
  assert.equal(await page.getByRole("button", { name: "Sign out", exact: true }).count(), 0);
  const focused = await page.evaluate(() => document.activeElement?.getAttribute("aria-label") ?? "");
  assert.match(focused, /^Account/);
});

await check("going live streams the playlist and counts its listener", async () => {
  await page.getByRole("switch", { name: /Go live/ }).click();
  await page.getByRole("switch", { name: /Live stream on/ }).waitFor({ timeout: 5_000 });
  const mp3 = await page.getByRole("button", { name: "Copy the MP3 link" }).getAttribute("data-tip");
  assert.match(mp3, /^https?:\/\/[^/]+\/live\/.+\/stream\.mp3$/, "the caption shows the whole link that gets copied");
  const got = await page.evaluate(async (url) => {
    const r = await fetch(url);
    const reader = r.body.getReader();
    const { value } = await reader.read();
    window.__liveReader = reader; // keep listening
    return { type: r.headers.get("content-type"), bytes: value.length };
  }, mp3);
  assert.equal(got.type, "audio/mpeg");
  assert.ok(got.bytes > 0);
  await page.locator(".listeners strong", { hasText: "1" }).waitFor({ timeout: 8_000 });
});

await check("the live strip changes when the server says so, never on a timer", async () => {
  const fetched = [];
  const onRequest = (r) => { if (/\/api\/v1\/playlists\/[^/]+\/live$/.test(r.url()) && r.method() === "GET") fetched.push(Date.now()); };
  page.on("request", onRequest);
  await page.evaluate(() => {
    window.__liveEvents = 0;
    new EventSource("/api/v1/events").addEventListener("live", () => window.__liveEvents++);
  });
  const mp3 = await page.getByRole("button", { name: "Copy the MP3 link" }).getAttribute("data-tip");
  await page.evaluate(async (url) => { window.__second = (await fetch(url)).body.getReader(); await window.__second.read(); }, mp3);
  await page.locator(".listeners strong", { hasText: "2" }).waitFor({ timeout: 3_000 });
  await page.waitForTimeout(11_000);
  page.off("request", onRequest);
  const events = await page.evaluate(() => window.__liveEvents);
  assert.ok(fetched.length <= events, `${fetched.length} fetches of the live state for ${events} live events`);
  await page.evaluate(() => window.__second.cancel());
});

await check("the Live switch's caption stays on screen at the left edge", async () => {
  await page.setViewportSize({ width: 720, height: 900 });
  const tip = await captionEdges(page.getByRole("switch", { name: /Live stream on/ }));
  await page.setViewportSize({ width: 1280, height: 900 });
  assert.ok(tip.left >= 0 && tip.right <= tip.width, `caption from ${tip.left} to ${tip.right} in ${tip.width}`);
});

await check("going private moves the stream to a private link", async () => {
  const before = await page.getByRole("button", { name: "Copy the MP3 link" }).getAttribute("data-tip");
  await page.getByRole("button", { name: /Make private/ }).click();
  await page.getByRole("button", { name: /Private: only people with the link/ }).waitFor({ timeout: 5_000 });
  const after = await page.getByRole("button", { name: "Copy the MP3 link" }).getAttribute("data-tip");
  assert.match(after, /^https?:\/\/[^/]+\/listen\//);
  assert.equal(await page.evaluate(async (u) => (await fetch(u)).status, before), 404);
});

await check("a live refresh keeps keyboard focus where it is", async () => {
  await page.evaluate(() => { document.querySelector(".track").__kept = 1; document.querySelector(".live-switch").__kept = 1; });
  await page.locator(".grip").first().focus();
  await page.waitForTimeout(6_000);
  assert.ok(await page.evaluate(() => document.activeElement?.classList.contains("grip")), "focus stays on the song's grip");
  assert.ok(await page.evaluate(() => document.querySelector(".track").__kept === 1), "the song rows are not rebuilt");
  assert.ok(await page.evaluate(() => document.querySelector(".live-switch").__kept === 1), "the live strip is not rebuilt");
  await page.getByRole("button", { name: /The stream plays in/ }).focus();
  await page.waitForTimeout(6_000);
  const focused = await page.evaluate(() => document.activeElement?.getAttribute("aria-label") ?? "");
  assert.match(focused, /The stream plays in/, "focus stays in the live strip");
});

await check("copy works without navigator.clipboard (plain http) and says when it fails", async () => {
  await page.evaluate(() => Object.defineProperty(navigator, "clipboard", { value: undefined, configurable: true }));
  await page.getByRole("button", { name: "Copy the MP3 link" }).click();
  await page.getByText("MP3 link copied.").first().waitFor({ timeout: 5_000 });
  await page.evaluate(() => {
    Object.defineProperty(navigator, "clipboard", { value: { writeText: () => Promise.reject(new Error("denied")) }, configurable: true });
    document.execCommand = () => false;
  });
  await page.getByRole("button", { name: "Copy the AAC link" }).click();
  await page.getByText(/Could not copy the AAC link/).first().waitFor({ timeout: 5_000 });
});

await check("what a toast says reaches screen readers", async () => {
  await page.getByRole("button", { name: "Copy the HLS link" }).click();
  await page.locator("[role=status]", { hasText: /HLS link/ }).waitFor({ state: "attached", timeout: 5_000 });
});

await check("Play on a live playlist tunes into its stream; a song's own Play stays on demand", async () => {
  const onAir = async () => (await (await page.request.get(base + "/api/v1" + playlistPath + "/live")).json()).on_air;
  assert.equal(await page.locator(".band").getByRole("button", { name: /^Shuffle/ }).count(), 0, "a live playlist's order is the live strip's shuffle");
  await page.locator(".band").getByRole("button", { name: /^Play / }).click();
  await page.locator(".player .live-mark").waitFor({ timeout: 5_000 });
  assert.match(await page.locator(".player audio").evaluate((a) => a.src), /\/listen\/[^/]+\/stream\.mp3$/);
  assert.ok(await page.locator(".player").getByRole("button", { name: /^Next song/ }).isDisabled(), "no skipping a broadcast");
  assert.ok(!(await page.getByRole("slider", { name: "Seek" }).isVisible()), "a stream has no seek bar");
  assert.ok(Number(await page.locator(".player").getByRole("button", { name: /^Next song/ }).evaluate((b) => getComputedStyle(b).opacity)) < 1, "a disabled control looks disabled");
  assert.ok(await page.locator(".player").getByRole("button", { name: /^Shuffle/ }).isDisabled(), "the stream's order is the live strip's to change");
  await page.locator(".now-title", { hasText: (await onAir()).now.title }).waitFor({ timeout: 8_000 });
  await page.locator(".band").getByRole("button", { name: /^Pause / }).waitFor({ timeout: 5_000 });
  await page.locator(".player").getByRole("button", { name: "Pause", exact: true }).click();
  await page.locator(".player").getByRole("button", { name: "Play", exact: true }).waitFor({ timeout: 3_000 });
  await page.locator(".band").getByRole("button", { name: /^Play / }).waitFor({ timeout: 3_000 });
  await page.locator(".player").getByRole("button", { name: "Play", exact: true }).click();
  await page.locator(".band").getByRole("button", { name: /^Pause / }).waitFor({ timeout: 5_000 });
  await page.locator(".track").first().getByRole("button", { name: /^Play / }).click();
  await page.locator(".player .live-mark").waitFor({ state: "detached", timeout: 5_000 });
  assert.match(await page.locator(".player audio").evaluate((a) => a.src), /\/songs\/[^/]+\/audio/);
  await page.getByRole("button", { name: "Pause", exact: true }).click();
});

await check("off air stops the stream, and a listening player says so", async () => {
  await page.locator(".band").getByRole("button", { name: /^Play / }).click();
  await page.locator(".player .live-mark").waitFor({ timeout: 5_000 });
  await page.getByRole("switch", { name: /Live stream on/ }).click();
  await page.getByRole("switch", { name: /Go live/ }).waitFor({ timeout: 5_000 });
  await page.locator("[role=status]", { hasText: /went off air/ }).waitFor({ state: "attached", timeout: 10_000 });
  await page.locator(".player .live-mark").waitFor({ state: "detached", timeout: 5_000 });
});

await check("the player bar shows only while it holds something, even on a live playlist's page", async () => {
  await page.locator(".player").waitFor({ state: "hidden", timeout: 5_000 });
  await page.getByRole("switch", { name: /Go live/ }).click();
  await page.getByRole("switch", { name: /Live stream on/ }).waitFor({ timeout: 5_000 });
  await page.waitForTimeout(1_000);
  assert.ok(!(await page.locator(".player").isVisible()), "no idle player bar on a live page");
  await page.locator(".band").getByRole("button", { name: /^Play / }).click();
  await page.locator(".player .live-mark").waitFor({ timeout: 5_000 });
  await page.locator(".player").getByRole("button", { name: "Pause", exact: true }).click();
  assert.ok(await page.locator(".player").isVisible(), "paused, it stays");
  await page.getByRole("switch", { name: /Live stream on/ }).click();
  await page.getByRole("switch", { name: /Go live/ }).waitFor({ timeout: 5_000 });
  await page.locator(".player").waitFor({ state: "hidden", timeout: 10_000 });
});

await check("a wrong current password says so and stays on the page", async () => {
  await page.getByRole("button", { name: /^Account/ }).click();
  await page.getByRole("button", { name: "Change password", exact: true }).click();
  await page.waitForURL(base + "/password");
  await page.getByLabel("Current password").fill("not-the-password");
  await page.locator("input[autocomplete=new-password]").first().fill("another-password-1");
  await page.getByLabel("New password again").fill("another-password-1");
  await page.getByRole("button", { name: "Change password", exact: true }).click();
  await page.locator(".error", { hasText: /./ }).waitFor({ timeout: 5_000 });
  assert.equal(new URL(page.url()).pathname, "/password");
  assert.equal((await page.locator(".error").textContent()).trim(), "That is not your current password.");
});

await check("a song list that fails to load says so, not 'No songs yet'", async () => {
  await page.route("**/api/v1/songs?**", (r) => r.fulfill({ status: 500, contentType: "application/json", body: '{"error":{"code":"internal","message":"something went wrong"}}' }));
  try {
    await page.getByRole("link", { name: /Maio/ }).click();
    await page.locator(".songs .error", { hasText: "Something went wrong" }).waitFor({ timeout: 5_000 });
    assert.equal(await page.getByText("No songs yet").count(), 0);
  } finally {
    await page.unroute("**/api/v1/songs?**");
  }
});

step("light theme survives a reload");
await page.goto(base + "/");
await page.waitForLoadState("networkidle");
await page.getByRole("button", { name: "Switch to light theme" }).click();
await page.reload();
await page.waitForLoadState("networkidle");
assert.equal(await page.evaluate(() => document.documentElement.dataset.theme), "light");

await check("on a wide screen the logo lines up with the page", async () => {
  await page.setViewportSize({ width: 1920, height: 900 });
  await page.waitForTimeout(300);
  const logo = await page.locator(".logo").first().boundingBox();
  const content = await page.locator(".page").first().evaluate((el) => el.getBoundingClientRect().left + parseFloat(getComputedStyle(el).paddingLeft));
  await page.setViewportSize({ width: 1280, height: 900 });
  assert.ok(Math.abs(logo.x - content) <= 1, `logo at ${logo.x}, content at ${content}`);
});

await check("in the light theme Live reads at 4.5:1 and the ready dot shows at 3:1", async () => {
  const got = await page.evaluate(() => {
    const row = document.createElement("div");
    row.className = "playlist-row";
    row.innerHTML = '<span class="live-chip">Live</span>';
    document.querySelector(".page").append(row);
    const bg = getComputedStyle(document.body).backgroundColor;
    const r = { live: getComputedStyle(row.firstChild).color, dot: getComputedStyle(document.querySelector(".engine.ready i")).backgroundColor, bg };
    row.remove();
    return r;
  });
  assert.ok(contrast(got.live, got.bg) >= 4.5, `Live ${got.live} on ${got.bg}: ${contrast(got.live, got.bg).toFixed(2)}`);
  assert.ok(contrast(got.dot, got.bg) >= 3, `ready dot ${got.dot} on ${got.bg}: ${contrast(got.dot, got.bg).toFixed(2)}`);
});

step("no sideways scrolling at 360 px");
await noSidewaysScroll("/");
await noSidewaysScroll(playlistPath);
await noSidewaysScroll("/playlists");

await check("the shelf never scrolls; with more playlists than it shows it ends with All playlists, level with the Make box", async () => {
  const headers = { origin: base, "content-type": "application/json" };
  const made = [];
  for (const name of ["Extra one", "Extra two", "Extra three", "Extra four", "Extra five"]) {
    const r = await page.request.post(base + "/api/v1/playlists", { headers, data: { name } });
    made.push((await r.json()).id);
  }
  try {
    await page.goto(base + "/");
    const all = page.getByRole("link", { name: /^All playlists/ });
    await all.waitFor({ timeout: 5_000 });
    assert.ok(await page.locator(".shelf a.tile:not(.all)").count() <= 5, "at most five playlists, then All playlists");
    const ends = await page.evaluate(() => {
      const shelf = document.querySelector(".shelf .tile.all").getBoundingClientRect().right;
      const make = document.querySelector(".make").getBoundingClientRect().right;
      return Math.abs(shelf - make) <= 6;
    });
    assert.ok(ends, "a full shelf ends where the Make box ends");
    const scrolls = () => page.evaluate(() => { const s = document.querySelector(".shelf"); return s.scrollWidth > s.clientWidth + 1; });
    assert.equal(await scrolls(), false, "no scrolling at full width");
    await page.setViewportSize({ width: 360, height: 800 });
    await page.waitForTimeout(300);
    assert.equal(await scrolls(), false, "no scrolling at 360 px");
    assert.ok(await all.isVisible(), "All playlists stays in view");
  } finally {
    await page.setViewportSize({ width: 1280, height: 900 });
    for (const id of made) await page.request.delete(base + `/api/v1/playlists/${id}`, { headers });
  }
});

await check("a dialog takes keyboard focus, keeps it, has a name and gives focus back", async () => {
  await page.goto(base + "/playlists");
  await page.waitForLoadState("networkidle");
  const np = page.getByRole("button", { name: "New playlist" });
  await np.focus();
  await page.keyboard.press("Enter");
  const d = page.getByRole("dialog", { name: "New playlist" });
  await d.waitFor({ timeout: 5_000 });
  const inside = () => d.evaluate((el) => el.contains(document.activeElement));
  assert.ok(await inside(), "focus moved into the dialog");
  assert.equal(await d.locator("h1 input, h2 input, h3 input").count(), 0, "the dialog's body is not a heading");
  // Past the last button Tab may reach the browser's own toolbar, never the page behind the dialog.
  const behind = () => d.evaluate((el) => { const a = document.activeElement; return !!a && a !== document.body && !el.contains(a); });
  for (let i = 0; i < 8; i++) {
    await page.keyboard.press("Tab");
    assert.ok(!(await behind()), `Tab ${i + 1} reached the page behind the dialog`);
  }
  await page.keyboard.press("Escape");
  await d.waitFor({ state: "hidden", timeout: 5_000 });
  assert.ok(await np.evaluate((el) => el === document.activeElement), "focus back on New playlist");
  const row = page.locator(".playlist-row").first();
  const more = row.locator(".dropdown > button");
  await more.click();
  await row.locator(".dropdown-list").getByRole("button", { name: "Rename" }).click();
  await page.getByRole("dialog", { name: "Rename playlist" }).waitFor({ timeout: 5_000 });
  await page.keyboard.press("Escape");
  await page.getByRole("dialog").waitFor({ state: "hidden", timeout: 5_000 });
  assert.ok(await more.evaluate((el) => el === document.activeElement), "focus back on the menu button");
});

await check("a search with no hits keeps the list valid and says how many songs it found", async () => {
  await page.goto(base + "/");
  await page.waitForLoadState("networkidle");
  await page.getByRole("searchbox").fill("zzzz-no-such-song");
  await page.getByText("No song matches that search.").waitFor({ timeout: 5_000 });
  assert.equal(await page.locator("[role=list]:has(> p)").count(), 0, "a list holding a paragraph");
  await page.locator("[role=status]", { hasText: "No songs found" }).waitFor({ state: "attached", timeout: 5_000 });
  await page.getByRole("searchbox").fill("");
});

await check("Add songs is a table of songs that fits the dialog with no scrollbar; a row's click ticks it", async () => {
  await page.goto(base + "/playlists");
  await page.waitForLoadState("networkidle");
  await page.getByRole("button", { name: "New playlist" }).click();
  await page.getByRole("dialog").getByLabel("Name").fill("Table test");
  await page.getByRole("dialog").getByRole("button", { name: "Create" }).click();
  await page.waitForURL(/\/playlists\/.+/);
  const id = new URL(page.url()).pathname.split("/").at(-1);
  await page.getByRole("button", { name: "Add songs" }).click();
  const dialog = page.getByRole("dialog", { name: "Add songs" });
  const table = dialog.getByRole("table");
  await table.locator("tbody tr").first().waitFor();
  assert.deepEqual((await table.locator("thead th").allInnerTexts()).map((t) => t.trim()).slice(1), ["Title", "Mood", "Key, tempo", "Length"]);
  const rows = table.locator("tbody tr");
  assert.ok((await rows.count()) >= 2, "every song in the library is offered");
  const scrolls = await dialog.evaluate((d) => [d, ...d.querySelectorAll("*")].some((el) => el.scrollHeight > el.clientHeight + 1 && /auto|scroll/.test(getComputedStyle(el).overflowY)));
  assert.ok(!scrolls, "nothing in the dialog scrolls");
  const title = (await rows.first().locator("td").nth(1).innerText()).trim();
  await rows.first().locator("td").nth(1).click();
  assert.ok(await dialog.getByRole("checkbox", { name: `Add ${title}` }).isChecked(), "a click on the row ticks it");
  await dialog.getByRole("button", { name: "Add 1 song" }).click();
  await page.locator(".track", { hasText: title }).waitFor();
  await page.request.delete(base + `/api/v1/playlists/${id}`, { headers: { origin: base } });
});

await check("the songs search shows its focus ring around the whole search pill, not inside it", async () => {
  await page.goto(base + "/");
  const box = page.getByRole("searchbox");
  await box.focus();
  await page.keyboard.press("a");
  const ring = await page.evaluate(() => {
    const input = document.querySelector(".search input"), pill = input.closest(".search");
    return { input: getComputedStyle(input).outlineStyle, pill: getComputedStyle(pill).outlineStyle };
  });
  assert.equal(ring.input, "none", "no ring on the field inside the pill");
  assert.equal(ring.pill, "solid", "the pill carries the ring");
  await box.fill("");
});

await check("the Make form's chips show their focus ring around the whole chip, not inside it", async () => {
  for (const name of ["Mood", "Genre", "Length", "Tempo"]) {
    await page.getByLabel(name, { exact: true }).focus();
    await page.keyboard.press("Shift");
    const ring = await page.evaluate(() => {
      const select = document.activeElement, chip = select.closest(".chip");
      return { select: getComputedStyle(select).outlineStyle, chip: getComputedStyle(chip).outlineStyle };
    });
    assert.deepEqual(ring, { select: "none", chip: "solid" }, `${name}: the chip carries the ring`);
  }
});

await check("the Playlists page renames and deletes a playlist from its row", async () => {
  await page.goto(base + "/playlists");
  await page.waitForLoadState("networkidle");
  await page.getByRole("button", { name: "New playlist" }).click();
  await page.getByRole("dialog").getByLabel("Name").fill("Rename me");
  await page.getByRole("dialog").getByRole("button", { name: "Create" }).click();
  await page.waitForURL(/\/playlists\/.+/);
  await page.goto(base + "/playlists");
  const row = page.locator(".playlist-row", { hasText: "Rename me" });
  await row.waitFor({ timeout: 5_000 });
  await row.getByRole("button", { name: "Rename or delete Rename me" }).click();
  await row.locator(".dropdown-list").getByRole("button", { name: "Rename" }).click();
  await page.getByRole("dialog").getByLabel("Name").fill("Renamed list");
  await page.getByRole("dialog").getByRole("button", { name: "Rename" }).click();
  const renamed = page.locator(".playlist-row", { hasText: "Renamed list" });
  await renamed.waitFor({ timeout: 5_000 });
  await renamed.getByRole("button", { name: "Rename or delete Renamed list" }).click();
  await renamed.locator(".dropdown-list").getByRole("button", { name: "Delete" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Delete" }).click();
  await renamed.waitFor({ state: "detached", timeout: 5_000 });
  assert.equal(new URL(page.url()).pathname, "/playlists", "deleting from the list stays on the list");
});

await check("a song row says which playlists hold it, and its mood and key stay on one line", async () => {
  await page.goto(base + "/");
  await page.locator(".song").first().waitFor();
  const rows = await page.locator(".song").evaluateAll((all) => all.map((r) => {
    const meta = r.querySelector(".meta"), name = r.querySelector(".marks-name");
    return { metaLines: Math.round(meta.getBoundingClientRect().height / parseFloat(getComputedStyle(meta).lineHeight || "20")), name: name ? name.textContent : null };
  }));
  for (const r of rows) assert.ok(r.metaLines <= 1, "a mood and key wrap");
  assert.ok(rows.some((r) => r.name === "Smoke test list"), `the playlist's name shows: ${JSON.stringify(rows.map((r) => r.name))}`);
});

await check("a session that ends while the page is open sends you to sign-in", async () => {
  await page.goto(base + "/");
  await page.waitForLoadState("networkidle");
  await page.context().clearCookies();
  await page.getByLabel("Search songs").fill("anything");
  await page.waitForURL(base + "/sign-in", { timeout: 5_000 });
});

await browser.close();
// The checks above load failing resources on purpose; only unexpected errors count.
const unexpected = problems.filter((p) => !/500|401|Failed to load resource/.test(p));
assert.deepEqual(unexpected, [], unexpected.join("\n"));
assert.deepEqual(failed, [], `failed checks: ${failed.join("; ")}`);
console.log("smoke test passed");
