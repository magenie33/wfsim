// A DEVICE IS COUNTED BY ITS TRAITS, AND COMPUTING OUT OF SIGHT IS MEASURED.
// What must hold (docs/ANALYTICS.md):
//   - `usageDevice` names the input, width and browser classes a device really
//     has: a desktop is `mouse_wide_browser`, the same page under touch and a
//     phone's width is `touch_narrow_…`, and WeChat's in-app browser says so;
//   - the cores go out rounded down to a step, never the exact count;
//   - `judgeBackground` says `kept` when a task finished out of sight,
//     `stopped` when the page made no beat, `idle` when it beat and finished
//     nothing, and nothing at all for a short absence.
//
//   node scripts/check_usage_device.mjs
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, send, sleep, BASE } = app;
await evaluate(`localStorage.clear(); localStorage.setItem('wfsim-lang', 'en')`);
await send("Page.navigate", { url: BASE });
await sleep(12000);

const desk = await evaluate(`usageDevice()`);
check("a desktop page is a mouse on a wide screen in a browser", desk.subject === "mouse_wide_browser", JSON.stringify(desk));
check("…its cores sent as a step, not the exact count",
  [1, 2, 4, 6, 8, 12, 16, 24, 32].includes(desk.cores), JSON.stringify(desk));

await send("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 3, mobile: true });
await send("Emulation.setTouchEmulationEnabled", { enabled: true, maxTouchPoints: 5 });
await send("Emulation.setUserAgentOverride", {
  userAgent: "Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Mobile Safari/537.36 MicroMessenger/8.0.50",
});
await sleep(500);
const phone = await evaluate(`usageDevice()`);
check("the same page under touch, a phone's width and WeChat is touch_narrow_wechat",
  phone.subject === "touch_narrow_wechat", JSON.stringify(phone));
await send("Emulation.clearDeviceMetricsOverride", {});
await send("Emulation.setTouchEmulationEnabled", { enabled: false });

// THE JUDGEMENT, driven by hand: the page is hidden and shown through a stubbed
// `document.hidden`, the absence aged past the window, and `track` read.
const said = await evaluate(`(() => {
  const out = [], sent = track;
  track = (e, s, n) => { if (e === 'compute.background') out.push(s + ':' + n); return sent(e, s, n); };
  let hidden = false;
  Object.defineProperty(document, 'hidden', { configurable: true, get: () => hidden });
  computeHolder = true;
  boardVerifyOn = () => true;
  onPhone = () => false;
  const away = (minutes, beats, finished) => {
    hidden = true; judgeBackground();
    hiddenFrom.at -= minutes * 60000;
    computeBeats += beats; computeFinished += finished;
    hidden = false; judgeBackground();
  };
  away(3, 0, 1);
  away(30, 4, 1);
  away(30, 0, 0);
  away(30, 9, 0);
  return out;
})()`);
check("out of sight long enough, the page says kept, stopped or idle by what it did, and nothing for a short absence",
  JSON.stringify(said) === JSON.stringify(["kept:30", "stopped:30", "idle:30"]), JSON.stringify(said));
process.exit(0);
