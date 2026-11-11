// Entity input identity is shared by the browser and the task factory.
const COMPUTATION_INPUT_SCHEMA = 1;
function computationInput(verb, value) {
  if (!["simulate", "simulate_report", "optimize", "optimize_report"].includes(verb)) return value;
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("a calculation requires entity inputs");
  const input = Object.fromEntries(Object.entries(value).filter(([key]) =>
    !key.startsWith("__") && !["engine", "release", "review", "threads", "input_schema"].includes(key)));
  if (verb === "optimize" && input.request) {
    input.request = computationInput("optimize_report", input.request);
    delete input.context;
  }
  return input;
}
const computationCanon = value => Array.isArray(value) ? `[${value.map(computationCanon).join(",")}]`
  : value && typeof value === "object" ? `{${Object.keys(value).sort().map(key => `${JSON.stringify(key)}:${computationCanon(value[key])}`).join(",")}}`
    : JSON.stringify(value);
async function computationId(verb, request) {
  const bytes = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(`${verb}\n${computationCanon(computationInput(verb, request))}`));
  return [...new Uint8Array(bytes)].map(byte => byte.toString(16).padStart(2, "0")).join("").slice(0, 32);
}

function computationReferences(value, aliases = {}) {
  if (typeof value === "string") return aliases[value] || value;
  if (Array.isArray(value)) return value.map(item => computationReferences(item, aliases));
  if (value && typeof value === "object") return Object.fromEntries(Object.entries(value)
    .map(([key, item]) => [aliases[key] || key, computationReferences(item, aliases)]));
  return value;
}
