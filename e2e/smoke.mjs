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

async function makeSong(prompt, seconds) {
  await page.getByLabel("What it sounds like").fill(prompt);
  await page.getByLabel("Length").selectOption(String(seconds));
  await page.getByRole("button", { name: "Make the song" }).click();
}

await check("the e2e config is committed, not ignored", async () => {
  let ignored = true;
  try { execSync("git check-ignore -q e2e/maio.toml", { cwd: ".." }); } catch { ignored = false; }
  assert.equal(ignored, false, "e2e/maio.toml is git-ignored");
});

step("strangers land on sign-in");
await page.goto(base + "/");
assert.equal(new URL(page.url()).pathname, "/sign-in");

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
await makeSong(longPrompt, 30);
await page.getByRole("heading", { name: "Being made" }).waitFor();
const firstSong = page.locator(".song").first();
await firstSong.waitFor({ timeout: 120_000 });
const title = (await firstSong.locator(".song-title").textContent()).trim();

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
  await page.getByRole("link", { name: /Smoke test list/ }).waitFor({ timeout: 5_000 });
  await page.getByRole("link", { name: /Smoke test list/ }).click();
  await page.waitForURL(base + playlistPath);
  await page.locator(".track").nth(1).waitFor();
});

step("the player kept its song across pages");
await page.locator(".now-title", { hasText: title }).waitFor();

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

step("no sideways scrolling at 360 px");
await noSidewaysScroll("/");
await noSidewaysScroll(playlistPath);
await noSidewaysScroll("/playlists");

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
