// AN INSTANT IS STORED as ISO 8601 to the millisecond (docs/NAMING.md §9), and
// a reply whose reader counts in epoch milliseconds converts at the boundary.
export const iso = (ms) => new Date(ms).toISOString();
export const msOf = (t) => (t === null || t === undefined ? t : typeof t === "number" ? t : Date.parse(t));
