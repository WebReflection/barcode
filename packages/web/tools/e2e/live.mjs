#!/usr/bin/env node
/**
 * Live release smoke: verify the PUBLISHED artifact, not the local build.
 *
 * Checks, in order:
 *   1. npm `dist-tags.latest` (the version this check follows);
 *   2. jsDelivr serves `dist/{main.js,worker.js,wasm.wasm,css.css}` for it;
 *   3. the deployed GitHub Pages site pins that version (badge + worker shim);
 *   4. headless Chromium renders with the CDN `main.js` + the site's shim.
 *
 * Run by hand after a publish, or nightly from .github/workflows/live.yml.
 * Never part of the per-PR gate: this one talks to npm, jsDelivr and Pages.
 *
 *   node tools/e2e/live.mjs
 *   node tools/e2e/live.mjs --version 0.2.3   # check a specific release
 *   node tools/e2e/live.mjs --no-render      # network checks only
 */

import { chromium } from "playwright";

const SITE = "https://webreflection.github.io/barcode/";
const PACKAGE = "@webreflection/barcode";
const CDN = (version, file) =>
  `https://cdn.jsdelivr.net/npm/${PACKAGE}@${version}/dist/${file}`;
const FILES = [
  ["main.js", 10_000],
  ["worker.js", 10_000],
  ["wasm.wasm", 1_000_000],
  ["css.css", 1_000],
];

const argv = process.argv.slice(2);
const has = (name) => argv.includes(`--${name}`);
const value = (name) => {
  const i = argv.indexOf(`--${name}`);
  return i >= 0 ? argv[i + 1] : undefined;
};
const RENDER = !has("no-render");

let passed = 0;
const failures = [];
function check(name, ok, detail = "") {
  if (ok) passed++;
  else failures.push(name);
  console.log(
    `${ok ? "PASS" : "FAIL"}  live  ${name.padEnd(26)} ${detail}`.trimEnd(),
  );
}

async function fetchOk(url, init = {}) {
  const res = await fetch(url, { cache: "no-store", ...init });
  if (!res.ok) return { res, text: null };
  return { res, text: await res.text() };
}

async function main() {
  let version = value("version");
  if (version) {
    check("version (--version)", true, version);
  } else {
    const { res, text } = await fetchOk(
      `https://registry.npmjs.org/${PACKAGE.replace("/", "%2f")}`,
      { headers: { accept: "application/vnd.npm.install-v1+json" } },
    );
    let latest;
    try {
      latest = JSON.parse(text);
    } catch {}
    version = latest?.["dist-tags"]?.latest;
    check(
      "npm dist-tags.latest",
      res.ok && typeof version === "string",
      version ?? `registry ${res.status}`,
    );
    if (typeof version !== "string") return;
  }

  for (const [file, min] of FILES) {
    const url = CDN(version, file);
    try {
      const res = await fetch(url, { cache: "no-store" });
      const size = res.ok
        ? new Uint8Array(await res.arrayBuffer()).byteLength
        : 0;
      check(`cdn ${file}`, res.ok && size >= min, `${res.status} ${size}B`);
    } catch (error) {
      check(`cdn ${file}`, false, String(error));
    }
  }

  {
    const { res, text } = await fetchOk(SITE);
    check(
      "pages badge",
      res.ok && text?.includes(`v${version}`),
      `${res.status}`,
    );
    const { res: shimRes, text: shim } = await fetchOk(
      `${SITE}worker-shim.js`,
    );
    check(
      "pages worker shim",
      shimRes.ok && shim?.includes(`${PACKAGE}@${version}/dist/worker.js`),
      `${shimRes.status}`,
    );
  }

  if (!RENDER) return;

  const browser = await chromium.launch();
  try {
    const page = await browser.newPage();
    await page.goto(SITE, { waitUntil: "load", timeout: 30_000 });
    const out = await page.evaluate(async (version) => {
      const mod = await import(
        `https://cdn.jsdelivr.net/npm/@webreflection/barcode@${version}/dist/main.js`
      );
      const { render } = mod.default({ worker: "./worker-shim.js" });
      const count = (svg) => svg?.querySelectorAll("rect").length ?? 0;
      const expanded = await render({
        symbology: "databarexpanded",
        text: "(01)40063813339314(10)ABC",
      });
      const qr = await render({ symbology: "qrcode", text: "LIVE-SMOKE" });
      return {
        expanded: { tag: expanded?.tagName, rects: count(expanded) },
        qr: count(qr),
      };
    }, version);
    check(
      "render databarexpanded",
      out.expanded.tag === "svg" && out.expanded.rects > 0,
      `${out.expanded.tag} ${out.expanded.rects} rects`,
    );
    check("render qrcode", out.qr > 0, `${out.qr} rects`);
  } finally {
    await browser.close();
  }
}

console.log(`# live smoke ${PACKAGE}`);
try {
  await main();
} catch (error) {
  check("unexpected error", false, String(error));
}
console.log(
  `== ${passed} PASS, ${failures.length} FAIL${failures.length ? ` (${failures.join(", ")})` : ""}`,
);
process.exitCode = failures.length ? 1 : 0;
