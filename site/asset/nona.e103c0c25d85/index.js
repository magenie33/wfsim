// NONA'S MOUNT. The page loads this module after `app.js`; it waits for the
// page's boot, then builds the agent on the door and mounts the panel. Nothing
// under `nona/` reaches the page any other way — docs/NONA.md §"The layers".

import { createAgent } from "./runtime/agent.js";
import { mountPanel } from "./ui/panel.js";

function mount() {
  const door = window.wfsim;
  mountPanel(door, createAgent(door));
}

// THE PAGE SAYS WHEN IT IS READY, and a module that loads after that reads it
// off the observation instead of waiting for an event that already fired.
if (window.wfsim && window.wfsim.observe().ready) mount();
else window.addEventListener("wfsim:ready", mount, { once: true });
