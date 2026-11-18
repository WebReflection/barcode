#!/usr/bin/env node
/**
 * Minimal CDP evaluator: `node tools/cdp_eval.mjs <url-substr> <js-expression>`
 * Finds the first phone/desktop page whose URL contains <url-substr>,
 * Runtime.evaluate()s the expression with awaitPromise, prints JSON result.
 */
const [, , urlSubstr, expr] = process.argv;
if (!urlSubstr || !expr) {
  console.error("usage: node tools/cdp_eval.mjs <url-substr> <js-expression>");
  process.exit(2);
}

const targets = await (await fetch("http://localhost:9222/json")).json();
const target = targets.find((t) => t.type === "page" && t.url.includes(urlSubstr));
if (!target) {
  console.error("no page matching", urlSubstr, "— have:", targets.map((t) => t.url));
  process.exit(1);
}

const ws = new WebSocket(target.webSocketDebuggerUrl);
let id = 0;
const pending = new Map();

function send(method, params) {
  return new Promise((resolve, reject) => {
    const msgId = ++id;
    pending.set(msgId, { resolve, reject });
    ws.send(JSON.stringify({ id: msgId, method, params }));
  });
}

ws.onmessage = (ev) => {
  const msg = JSON.parse(ev.data);
  if (msg.id && pending.has(msg.id)) {
    const { resolve, reject } = pending.get(msg.id);
    pending.delete(msg.id);
    msg.error ? reject(new Error(JSON.stringify(msg.error))) : resolve(msg.result);
  }
};

await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });

const result = await send("Runtime.evaluate", {
  expression: expr,
  awaitPromise: true,
  returnByValue: true,
});
if (result.exceptionDetails) {
  console.error("EXCEPTION:", JSON.stringify(result.exceptionDetails, null, 2));
  process.exit(1);
}
console.log(JSON.stringify(result.result.value, null, 2));
ws.close();
process.exit(0);
