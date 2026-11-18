/**
 * Encoder worker: owns the WASM barcode encoders and produces fixture
 * bitmaps for re-presentation (createSVG). Kept separate from the
 * pipeline worker so SVG rendering works with or without a live Scanner.
 *
 * Protocol (same bootstrap pattern as the pipeline worker): the main
 * thread posts "init" immediately after `new Worker` — messages queue
 * until this script runs — and this worker posts back "ready" once the
 * WASM is up. Encode requests are answered strictly serially (one
 * "encode" in flight), so no request ids are needed.
 */

import init, { encode_fixture_opts } from "./wasm/pkg/barcode_wasm.js";
import wasmUrl from "./wasm/pkg/barcode_wasm_bg.wasm?url";

const wasmOverride = (() => {
  try {
    return new URL(import.meta.url).searchParams.get("wasm");
  } catch {
    return null;
  }
})();

/** The wasm serde boundary is untyped; this is its actual shape. */
/** @typedef {{ width: number, height: number, bytesPerRow: number, data: Uint8Array | number[] }} Fixture */

/** Wire protocol (svg.js is the other end). */
/** @typedef {{ type: "init" }} InitRequest */
/** @typedef {{
 *   type: "encode",
 *   symbology: string,
 *   text: string,
 *   module?: number,
 *   height?: number,
 *   ec?: number,
 *   mask?: number,
 *   version?: number,
 *   level?: number,
 *   columns?: number,
 *   rows?: number,
 *   cols?: number,
 * }} EncodeRequestMsg */
/** @typedef {{ type: "ready" }} ReadyMsg */
/** @typedef {{ type: "error", message: string }} InitErrorMsg */
/** @typedef {{ type: "encoded", width: number, height: number, bytesPerRow: number, data: Uint8Array }} EncodedMsg */
/** @typedef {{ type: "encodeError", message: string }} EncodeErrorMsg */
/** @typedef {ReadyMsg | InitErrorMsg | EncodedMsg | EncodeErrorMsg} EncodeReply */

/**
 * Worker-scope postMessage (the tsconfig DOM lib types `self` as
 * Window, whose postMessage signature has no transfer list).
 * @param {EncodeReply} msg
 * @param {Transferable[]} [transfer]
 */
const post = (msg, transfer = []) => {
  (/** @type {{ postMessage: (m: EncodeReply, t?: Transferable[]) => void }} */ (self)).postMessage(msg, transfer);
};

self.onmessage = async (/** @type {MessageEvent} */ e) => {
  const msg = /** @type {InitRequest | EncodeRequestMsg} */ (e.data);
  if (msg?.type === "init") {
    try {
      await init(wasmOverride || wasmUrl);
      post({ type: "ready" });
    } catch (err) {
      post({ type: "error", message: String(err) });
    }
    return;
  }
  if (msg?.type === "encode") {
    try {
      const module = msg.module ?? 4;
      const fx = /** @type {Fixture} */ (
        encode_fixture_opts(
          msg.symbology, msg.text, module, msg.height ?? 30 * module,
          msg.ec ?? -1, msg.mask ?? -1, msg.version ?? 0,
          msg.level ?? -1, msg.columns ?? 0, msg.rows ?? 0, msg.cols ?? 0,
        )
      );
      // serde may hand back a Uint8Array or a plain array — normalize so
      // the buffer is transferable.
      const data =
        fx.data instanceof Uint8Array ? fx.data : Uint8Array.from(fx.data);
      post(
        {
          type: "encoded",
          width: fx.width,
          height: fx.height,
          bytesPerRow: fx.bytesPerRow,
          data,
        },
        [data.buffer],
      );
    } catch (err) {
      post({ type: "encodeError", message: String(err) });
    }
  }
};
