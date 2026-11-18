#!/usr/bin/env node
/**
 * Serve the built demo for real-device testing.
 *
 *   node tools/serve-demo.mjs            # http://<lan-ip>:8124 (Android)
 *   node tools/serve-demo.mjs --https    # https://<lan-ip>:8443 (iPhone)
 *   node tools/serve-demo.mjs --docs     # the GitHub Pages site in docs/
 *
 * The built demo lives in `demo-dist/` (`pnpm build:demo`). Android over
 * USB is better served by the Vite dev server + `adb reverse`, but this
 * script works on any device on the LAN.
 *
 * Cameras require a secure origin: `http://localhost` is fine (adb
 * reverse), a LAN IP needs HTTPS. `--https` uses mkcert when available
 * and prints how to trust its CA on the iPhone. The certificate and key
 * stay under the gitignored `.certs/`.
 */
import { execFileSync } from "node:child_process";
import { createServer } from "node:https";
import { createServer as createHttpServer } from "node:http";
import { existsSync, mkdirSync, readFileSync, statSync } from "node:fs";
import { networkInterfaces } from "node:os";
import { extname, join, normalize, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(fileURLToPath(new URL("..", import.meta.url)));
const argv = process.argv.slice(2);
const HTTPS = argv.includes("--https");
const DOCS = argv.includes("--docs");
const dist = resolve(root, DOCS ? "../../docs" : "demo-dist");
const entry = DOCS ? "index.html" : "scan.html";
const PORT = Number(
  argv.find((a, i) => argv[i - 1] === "--port") ?? (HTTPS ? 8443 : 8124),
);
if (!existsSync(resolve(dist, entry))) {
  console.error(
    `missing ${dist}/${entry} — build it first: ` +
    (DOCS ? "npm run build:docs" : "npm run build:demo"),
  );
  process.exit(2);
}

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".wasm": "application/wasm",
  ".json": "application/json",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".jpeg": "image/jpeg",
  ".ico": "image/x-icon",
};
/** @param {import('node:http').IncomingMessage} req @param {import('node:http').ServerResponse} res */
const handler = (req, res) => {
  try {
    const url = new URL(req.url ?? "/", "http://localhost");
    let path = normalize(join(dist, decodeURIComponent(url.pathname)));
    if (!path.startsWith(dist)) {
      res.writeHead(403).end("forbidden");
      return;
    }
    if (statSync(path).isDirectory()) path = join(path, "index.html");
    res.writeHead(200, {
      "content-type": MIME[extname(path)] ?? "application/octet-stream",
      "cache-control": "no-store",
    });
    res.end(readFileSync(path));
  } catch {
    res.writeHead(404).end("not found");
  }
};

let server;
if (HTTPS) {
  const certs = resolve(root, ".certs");
  mkdirSync(certs, { recursive: true });
  const cert = resolve(certs, "localhost.pem");
  const key = resolve(certs, "localhost-key.pem");
  if (!existsSync(cert) || !existsSync(key)) {
    try {
      execFileSync("mkcert", [
        "-cert-file", cert, "-key-file", key,
        "localhost", "127.0.0.1", "::1",
        ...lanAddresses(),
      ], { stdio: "inherit" });
    } catch {
      console.error(
        "HTTPS needs mkcert (brew install mkcert && mkcert -install),\n" +
        "or drop your own localhost.pem / localhost-key.pem into .certs/",
      );
      process.exit(2);
    }
  }
  server = createServer({ cert: readFileSync(cert), key: readFileSync(key) }, handler);
} else {
  server = createHttpServer(handler);
}

await new Promise((r) => server.listen(PORT, "0.0.0.0", r));

const scheme = HTTPS ? "https" : "http";
console.log(`serving ${dist}`);
console.log(`  local:   ${scheme}://localhost:${PORT}/${entry}`);
for (const ip of lanAddresses()) {
  console.log(`  network: ${scheme}://${ip}:${PORT}/${entry}`);
}
if (HTTPS) {
  try {
    console.log(`\nmkcert CA: ${execFileSync("mkcert", ["-CAROOT"]).toString().trim()}/rootCA.pem`);
    console.log("Install + trust that root CA once on the iPhone (AirDrop it, then Settings → General → VPN & Device Management → install, and Settings → General → About → Certificate Trust Settings → enable).");
  } catch { /* mkcert not installed: certs were provided by hand */ }
}
console.log("\nCtrl+C to stop.");

/** @returns {string[]} */
function lanAddresses() {
  /** @type {string[]} */
  const out = [];
  for (const list of Object.values(networkInterfaces())) {
    for (const iface of list ?? []) {
      if (iface.family === "IPv4" && !iface.internal) out.push(iface.address);
    }
  }
  return out;
}
