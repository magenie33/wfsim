// THE LONG IMAGE THE BOTS SEND (45-card-page.js): each kind of card draws its
// cards, its code and every riven's values, and says it is ready — the moment
// the bot server's screenshot is taken — with nothing else on the page.
//   node scripts/check_card_page.mjs            (WFSIM_BASE=http://127.0.0.1:8787 against a dev server)
import { openApp, sleep } from "./cdp.mjs";

const app = await openApp({ boot: 13000, base: process.env.WFSIM_BASE });
const { evaluate, check, finish } = app;
const ready = async () => {
  for (let i = 0; i < 60; i++) {
    if (await evaluate(`document.body.dataset.cardReady === "1"`)) return true;
    await sleep(500);
  }
  return false;
};

await app.load("/weapons/Furis/card?kind=pz&n=3", 4000);
check("a build card says it is ready", await ready());
check("…and draws the three builds asked for", (await evaluate(`document.querySelectorAll("#card-page .lc-box").length`)) === 3);
check("…with its code", await evaluate(`!!document.querySelector("#card-page .lc-qr svg")`));
check("…kept the question its address asked", (await evaluate(`location.search`)).includes("kind=pz"));
check("…and is the only thing on the page", (await evaluate(`[...document.body.children].filter((e) => e.id !== "card-page" && e.getClientRects().length).length`)) === 0);

await app.load("/weapons/Torid/card?kind=zk&n=2", 4000);
check("a riven card says it is ready", await ready());
check("…draws the riven-free leader and two rivens", (await evaluate(`document.querySelectorAll("#card-page .lc-box").length`)) === 3);
const chips = await evaluate(`[...document.querySelectorAll("#card-page [data-riven-card] .sb-chip")].map((c) => c.textContent)`);
check("…and every riven stat carries its value, not only its name", chips.length >= 6 && chips.every((t) => /\d+(\.\d+)?%|x\d/.test(t)), JSON.stringify(chips.slice(0, 4)));
// RED IS THE MALUS, not a minus: recoil's malus prints +51% and its bonus -51%.
const red = await evaluate(`(async () => {
  const spec = { rank: 8, polarity: "madurai", bonuses: [{ id: "multishot", roll: 1 }, { id: "weapon_recoil", roll: 1 }],
    malus: { id: "zoom", roll: 1 } };
  rivenSpecCardHtml(weaponInfo("torid"), { name: "t", spec });
  await new Promise((r) => setTimeout(r, 2500));
  const box = document.createElement("div");
  box.innerHTML = rivenSpecCardHtml(weaponInfo("torid"), { name: "t", spec });
  return [...box.querySelectorAll(".sb-chip")].map((c) => (c.classList.contains("neg") ? "red " : "") + c.textContent);
})()`);
check("…and the red one is the malus, whatever its sign", red.length === 3 && red.filter((t) => t.startsWith("red ")).length === 1
  && /-.*%/.test(red[2]) && red[2].startsWith("red ") && /-/.test(red[1]) && !red[1].startsWith("red "), JSON.stringify(red));

await finish("the long image is ready when it says so");
