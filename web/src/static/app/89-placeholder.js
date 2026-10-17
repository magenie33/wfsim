// ---- A CATEGORY THE BUILDER DOES NOT HOLD YET ------------------------------
//
// `holds: placeholder` in data/equipment_slots.yaml: the category has its card
// on the home grid and its page at `/<name>`, which lists the cards it takes
// (`/api/catalogue`) and simulates none of them.
// docs/UI.md §A category the builder does not hold yet.
const placeholderSlots = () => ((META && META.equipment_slots) || []).filter((sl) => sl.holds === "placeholder");
/// THE GAME'S OWN NAME, lowercased, as `/operator` is: "Arch-Melee" is
/// `/arch-melee`. The address outlives the placeholder, so it is not the id.
const placeholderPath = (slot) => "/" + String(slot.name).toLowerCase().replace(/ /g, "-");
const CATALOGUE_HEADING = { mod: "Mods", arcane: "Arcanes", avionic: "Avionics" };
let catalogue = null;

async function renderPlaceholder(slot) {
  $("slot-name").textContent = tr(slot.name);
  const block = $("slot-cards-block");
  if (!catalogue) {
    block.hidden = true;
    try { catalogue = await api("/api/catalogue", {}); } catch (e) { return; }
  }
  const c = catalogue[slot.id];
  block.hidden = !c || !c.cards.length;
  if (block.hidden) return;
  const zhName = (I18N && I18N.catalogue) || {};
  const zhText = (I18N && I18N.catalogue_descriptions) || {};
  $("slot-cards-h").textContent = tr(CATALOGUE_HEADING[c.kind] || "Mods");
  $("slot-cards-n").textContent = String(c.cards.length);
  $("slot-cards").innerHTML = c.cards.map((x) => {
    const key = `${slot.id}/${x.id}`;
    const pol = x.polarity && x.polarity !== "universal" ? " " + polGlyph(x.polarity) : "";
    const lines = String(zhText[key] || x.description).split("\n").filter(Boolean).map(escHtml);
    return `<div class="opt rar-${escHtml(x.rarity || "common")}"><div class="info">`
      + `<div class="mn">${escHtml(zhName[key] || x.name)}${pol}</div>${effLines(lines)}</div></div>`;
  }).join("");
}
