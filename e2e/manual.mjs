// Clips for the guide to the web app (docs/how-to/07-use-the-web-app.md) and the tutorial: SCREENS=1 just e2e runs
// this after the smoke test, against the same server. Each guide is recorded in its own browser; each step becomes a
// short animated PNG cropped to the part of the screen it uses, with a pointer that moves to each control, and a
// still of its last frame.
// Commit them when the web app changes how it looks.
import { chromium } from "playwright";
import { mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { tmpdir } from "node:os";
import path from "node:path";

const base = process.env.E2E_URL ?? "http://127.0.0.1:8099";
const out = new URL("../docs/images/manual/", import.meta.url).pathname;
const raw = mkdtempSync(path.join(tmpdir(), "maio-manual-"));
const VIEW = { width: 1280, height: 800 };
const SCALE = 1; // Playwright records at the viewport's CSS size, whatever the device scale
const MIN = { width: 560, height: 340 }; // a clip shows at least this much around where the pointer went
mkdirSync(out, { recursive: true });

// Recordings show no cursor: draw one, outside <body> so hydration leaves it alone.
const POINTER = `addEventListener("DOMContentLoaded", () => {
  const c = document.createElement("div");
  Object.assign(c.style, { position: "fixed", left: "640px", top: "400px", width: "20px", height: "20px", margin: "-10px 0 0 -10px", borderRadius: "50%",
    background: "rgba(232,163,61,.9)", border: "2px solid #fff", boxShadow: "0 1px 6px rgba(0,0,0,.45)", pointerEvents: "none", zIndex: 2147483647, transition: "transform .12s" });
  document.documentElement.append(c);
  addEventListener("mousemove", (e) => { c.style.left = e.clientX + "px"; c.style.top = e.clientY + "px"; }, true);
  addEventListener("mousedown", () => { c.style.transform = "scale(.65)"; }, true);
  addEventListener("mouseup", () => { c.style.transform = ""; }, true);
});`;

const browser = await chromium.launch();
const signedIn = await (async () => {
  const context = await browser.newContext({ viewport: VIEW });
  const page = await context.newPage();
  await page.goto(base + "/sign-in");
  await page.waitForLoadState("networkidle"); // fill the form once the page is live, so Sign in is enabled
  await page.getByLabel("Username").fill(process.env.E2E_USER);
  await page.getByLabel("Password").fill(process.env.E2E_PASSWORD);
  await page.getByRole("button", { name: "Sign in" }).click();
  await page.waitForURL(base + "/");
  const state = await context.storageState();
  await context.close();
  return state;
})();

const PALETTE = "split[a][b];[a]palettegen=max_colors=256:stats_mode=diff[p];[b][p]paletteuse=dither=none:diff_mode=rectangle";
const ffmpeg = (...args) => execFileSync("ffmpeg", ["-v", "error", "-y", ...args]);
const even = (n) => Math.floor(n / 2) * 2;

/** Records one guide: `fn` gets the page and `step(name, areas, body)`, where areas say what to crop to ("page" for all). */
// MANUAL_ONLY=overview,make re-records only those guides.
const only = process.env.MANUAL_ONLY?.split(",");
async function guide(name, fn) {
  if (only && !only.includes(name)) return;
  console.log(`- ${name}`);
  const context = await browser.newContext({ viewport: VIEW, storageState: signedIn, recordVideo: { dir: raw, size: { width: VIEW.width * SCALE, height: VIEW.height * SCALE } } });
  await context.addInitScript(POINTER);
  const page = await context.newPage();
  const t0 = Date.now();
  const steps = [];
  let aimed = [];
  const boxesOf = async (areas) => {
    if (areas === "page" || areas === "full") return [{ x: 0, y: 0, width: VIEW.width, height: VIEW.height }];
    const boxes = [];
    for (const area of areas()) if (await area.count() && await area.first().isVisible()) boxes.push(await area.first().boundingBox());
    return boxes;
  };
  const step = async (n, areas, body) => {
    await page.waitForTimeout(300);
    const start = (Date.now() - t0) / 1000;
    aimed = [];
    const before = await boxesOf(areas);
    await body();
    await page.waitForTimeout(1500); // the recording's clock runs a little behind: let the result show for a while
    const boxes = [...before, ...(await boxesOf(areas))].filter(Boolean);
    steps.push({ n, start, end: (Date.now() - t0) / 1000, boxes, aimed, full: areas === "full" }); // "full": the whole screen, wherever the pointer went
  };
  // Moves the pointer to a control the way a person would, then uses it.
  const aim = async (locator) => {
    await locator.scrollIntoViewIfNeeded();
    const b = await locator.boundingBox();
    aimed.push(b);
    await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2, { steps: 24 });
    await page.waitForTimeout(250);
  };
  const click = async (locator) => { await aim(locator); await locator.click(); };
  // What a step changes, kept in its clip, though the pointer never goes there: the lyrics box that opens, the job.
  const look = async (locator) => { await locator.first().waitFor(); aimed.push(await locator.first().boundingBox()); };
  const type = async (locator, text) => { await click(locator); await locator.fill(""); await locator.pressSequentially(text, { delay: 45 }); };
  await page.mouse.move(VIEW.width / 2, VIEW.height / 2);
  await fn({ page, step, aim, click, type, look });
  await page.waitForTimeout(800); // so the recording runs past the last step's last frame
  const video = page.video();
  await context.close();
  const file = await video.path();
  for (const { n, start, end, boxes, aimed, full } of steps) {
    // Crop to what the step touched, grown to at least MIN around it and kept inside the step's areas and the screen.
    const box = (list) => { const x = Math.min(...list.map((b) => b.x)), y = Math.min(...list.map((b) => b.y)); return { x, y, w: Math.max(...list.map((b) => b.x + b.width)) - x, h: Math.max(...list.map((b) => b.y + b.height)) - y }; };
    const area = box(boxes), focus = aimed.length && !full ? box(aimed) : area, pad = 20;
    const grow = (at, size, min, lo, hi) => { const want = Math.max(size + 2 * pad, min), from = Math.max(lo, Math.min(at + size / 2 - want / 2, hi - want)); return [Math.max(0, from), Math.min(want, hi - Math.max(0, from))]; };
    const [x, w] = grow(focus.x, focus.w, MIN.width, Math.max(0, area.x - pad), Math.min(VIEW.width, area.x + area.w + pad));
    const [y, h] = grow(focus.y, focus.h, MIN.height, Math.max(0, area.y - pad), Math.min(VIEW.height, area.y + area.h + pad));
    const crop = `crop=${even(w * SCALE)}:${even(h * SCALE)}:${Math.round(x * SCALE)}:${Math.round(y * SCALE)}`;
    const clip = path.join(out, `${name}-${n}`);
    // An animated PNG plays anywhere an image shows, GitHub included: 8 frames a second, at most 720 px wide, the last
    // frame held 1.5 s before it loops. The still is that last frame, for anyone who asked for less motion.
    const scale = "scale='min(720,iw)':-2:flags=lanczos";
    // Seek after decoding: Playwright's WebM has few keyframes, and a seek before the input shows grey until the next one.
    // A palette of the clip's own 256 colours keeps a flat interface sharp at about a fifth of the size of full colour.
    ffmpeg("-i", file, "-ss", String(Math.max(0, start - 0.1)), "-to", String(end), "-filter_complex", `${crop},fps=8,${scale},tpad=stop_mode=clone:stop_duration=1.5,${PALETTE}`, "-plays", "0", "-f", "apng", `${clip}.png`);
    // The still is the clip's own last frame, so it always shows what the clip ends on.
    ffmpeg("-i", `${clip}.png`, "-vf", "reverse", "-frames:v", "1", "-update", "1", `${clip}-still.png`);
    console.log(`  ${name}-${n}: ${(end - start).toFixed(1)} s, ${Math.round(w)}×${Math.round(h)}`);
  }
}

const songs = (page) => page.locator(".song");
const first = (page) => songs(page).first();

// The README's picture of Maio: a song described, made and played, the whole screen.
await guide("overview", async ({ page, step, click, type, look }) => {
  await page.goto(base + "/");
  await page.locator("form.make").waitFor();
  await step(1, "full", async () => {
    await type(page.getByLabel("What it sounds like"), "slow piano and rain on the window, a late night");
    await click(page.getByRole("button", { name: /^Song settings/ }));
    await page.getByRole("slider", { name: "Length" }).fill("30");
    await click(page.getByRole("button", { name: "Make the song" }));
    await page.getByRole("heading", { name: "Being made" }).waitFor({ state: "hidden", timeout: 60_000 });
    await click(first(page).locator(".play"));
    await look(page.locator(".player"));
  });
});

await guide("make", async ({ page, step, click, type, look }) => {
  await page.goto(base + "/");
  const make = () => [page.locator("form.make")];
  await step(1, make, () => type(page.getByLabel("What it sounds like"), "warm pads, slow, rain on the window"));
  await step(2, make, async () => {
    await click(page.getByRole("button", { name: /^Song settings/ }));
    await click(page.getByLabel("Mood", { exact: true }));
    await page.getByLabel("Mood", { exact: true }).selectOption("calm");
    await page.keyboard.press("Escape");
  });
  await step(3, make, async () => {
    await page.getByRole("combobox", { name: /^Vocals/ }).selectOption("vocals");
    await look(page.locator("form.make .lyrics"));
  });
  await step(4, () => [page.locator("form.make"), page.locator("section.making")], async () => {
    await page.getByRole("slider", { name: "Length" }).fill("30");
    await click(page.getByRole("button", { name: "Make the song" }));
    await look(page.locator("section.making .job"));
  });
  await step(5, () => [page.locator("section.making"), first(page)], async () => {
    await page.getByRole("heading", { name: "Being made" }).waitFor({ state: "hidden", timeout: 60_000 });
    await look(first(page));
  });
});

await guide("again", async ({ page, step, click, look }) => {
  await page.goto(base + "/");
  await first(page).waitFor();
  await step(1, () => [first(page), page.getByRole("dialog")], async () => { await click(first(page).locator(".song-title")); await look(page.getByRole("dialog")); });
  await step(2, () => [page.locator("form.make")], async () => { await click(page.getByRole("dialog").getByRole("button", { name: "Make one like this" })); await look(page.locator("form.make .set.seed")); });
  await step(3, () => [page.locator("form.make"), page.locator("section.making")], async () => {
    await click(page.getByRole("button", { name: "Make the song" }));
    await look(page.locator("section.making .job"));
  });
});

await guide("play", async ({ page, step, aim, click, type, look }) => {
  await page.goto(base + "/");
  await first(page).waitFor();
  await step(1, () => [page.locator(".list-head"), page.locator(".songs")], async () => { await type(page.getByLabel("Search songs"), "rain"); await look(first(page)); });
  await page.getByLabel("Search songs").fill("");
  await step(2, () => [first(page), page.locator(".player")], async () => { await click(first(page).locator(".play")); await look(page.locator(".player")); });
  await step(3, () => [page.locator(".player")], async () => {
    await click(page.getByRole("button", { name: /^Repeat/ }));
    await click(page.locator(".player").getByRole("button", { name: /^Pause|^Play$/ }));
  });
  await step(4, () => [page.getByRole("dialog")], async () => {
    await click(first(page).locator(".song-title"));
    await page.waitForTimeout(400);
    await aim(page.getByRole("dialog").getByRole("button", { name: "Download as MP3" }).or(page.getByRole("dialog").getByRole("link", { name: "Download as MP3" })).first());
    await look(page.getByRole("dialog").locator(".downloads"));
  });
});

await guide("rename", async ({ page, step, click, type, look }) => {
  await page.goto(base + "/");
  await first(page).waitFor();
  const title = (await first(page).locator(".song-title").textContent()).trim();
  await step(1, () => [first(page), page.locator(".dropdown-list:visible")], () => click(first(page).getByRole("button", { name: `Details, rename or delete ${title}` })));
  await step(2, () => [page.getByRole("dialog"), first(page)], async () => {
    await click(page.locator(".dropdown-list:visible").getByRole("button", { name: /Rename/ }).first());
    await type(page.getByRole("dialog").getByLabel("Title"), "Rain on the window");
    await click(page.getByRole("dialog").getByRole("button", { name: "Rename" }));
    await look(first(page));
  });
});

await guide("playlist", async ({ page, step, click, type, look }) => {
  await page.goto(base + "/");
  await first(page).waitFor();
  await step(1, "page", async () => {
    await click(page.locator("section.shelf .tile.new"));
    await type(page.getByRole("dialog").getByLabel("Name"), "Rainy evening");
    await click(page.getByRole("dialog").getByRole("button", { name: "Create" }));
    await page.waitForURL(/\/playlists\//);
  });
  const playlist = page.url();
  await page.goto(base + "/");
  await first(page).waitFor();
  const title = (await first(page).locator(".song-title").textContent()).trim();
  await step(2, () => [first(page), page.locator(".dropdown-list:visible")], async () => {
    await click(first(page).getByRole("button", { name: `Add ${title} to a playlist` }));
    await look(page.locator(".dropdown-list:visible"));
    await click(page.locator(".dropdown-list:visible").getByRole("button", { name: "Rainy evening" }));
  });
  await page.goto(playlist);
  await page.locator(".track").first().waitFor();
  await step(3, "page", async () => {
    await click(page.getByRole("button", { name: "Add songs" }));
    await click(page.getByRole("dialog").locator("tbody tr").first());
    await click(page.getByRole("dialog").getByRole("button", { name: /^Add \d/ }));
    await page.locator(".track").nth(1).waitFor();
  });
  await step(4, () => [page.locator(".tracks")], async () => {
    await click(page.locator(".track .grip").first());
    await page.keyboard.press("ArrowDown");
    await look(page.locator(".tracks"));
  });
});

await guide("live", async ({ page, step, click, look }) => {
  await page.goto(base + "/playlists");
  await click(page.getByRole("link", { name: /Rainy evening/ }).first());
  await page.locator(".live-strip").waitFor();
  const strip = () => [page.locator(".band"), page.locator(".live-strip")];
  await step(1, strip, async () => {
    await click(page.getByRole("switch", { name: /Go live/ }));
    await page.getByRole("switch", { name: /Live stream on/ }).waitFor({ timeout: 8_000 });
  });
  await step(2, strip, () => click(page.getByRole("button", { name: "Copy the MP3 link" })));
  await step(3, strip, () => click(page.getByRole("switch", { name: /Live stream on/ })));
});

await guide("engine", async ({ page, step, click, look }) => {
  await page.goto(base + "/");
  await step(1, () => [page.locator("header.top"), page.locator(".engines")], async () => { await click(page.locator("button.engine").first()); await look(page.locator(".engines")); });
});

await guide("account", async ({ page, step, click, look }) => {
  await page.goto(base + "/");
  const header = () => [page.locator("header.top")];
  await step(1, header, async () => {
    await click(page.getByRole("button", { name: /Switch to (light|dark) theme/ }));
    await page.waitForTimeout(600);
    await click(page.getByRole("button", { name: /Switch to (light|dark) theme/ }));
  });
  await step(2, "page", async () => {
    await click(page.getByRole("button", { name: "Account: change password or sign out" }));
    await click(page.locator(".dropdown-list:visible").getByRole("button", { name: "Change password" }).or(page.locator(".dropdown-list:visible").getByRole("link", { name: "Change password" })).first());
    await page.waitForURL(/\/password$/);
    await look(page.getByLabel("Current password"));
  });
});

await browser.close();
rmSync(raw, { recursive: true, force: true });
console.log("clips written to docs/images/manual");
