// THE EDGE WORKER — wfsim.app's front door, run where the reader is.
//
// The site's own files (worker/pages.js) are answered here; everything that
// needs a database goes to the placed worker (worker/index.js) through the
// BACKEND service binding. One worker cannot be both: Smart Placement moves the
// WHOLE script near its Postgres, and with it the page and the engine every
// reader downloads — docs/WASM.md §"Two workers".
import { edgeServe } from "./pages.js";

export default {
  async fetch(request, env) {
    return (await edgeServe(request, env)) || env.BACKEND.fetch(request);
  },
};
