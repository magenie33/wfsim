// The shadow check's loader hook (scripts/pg/shadow.mjs): `node:sqlite` is the
// shadow database everywhere but inside it.
const SHADOW = new URL("./shadow_sqlite.mjs", import.meta.url).href;
export async function resolve(specifier, context, next) {
  if (specifier === "node:sqlite" && context.parentURL !== SHADOW) return { url: SHADOW, shortCircuit: true };
  return next(specifier, context);
}
