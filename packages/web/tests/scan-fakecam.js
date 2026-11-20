/**
 * Internal fake-camera page. It is the public scan page (same markup, same
 * session module) plus the single hook the e2e mock needs: draw a specimen
 * through the page's own encoder on the page's own worker.
 */
import { render } from "../demo/scan.js";

/** @type {any} */ (globalThis).__barcodeRender = (
  /** @type {{ type: string, value: string }} */ { type, value },
) => render({ symbology: type, text: value });
