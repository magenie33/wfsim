// AN INSTANT IS STORED as ISO 8601 to the millisecond (docs/NAMING.md §9), and
// a reply whose reader counts in epoch milliseconds converts at the boundary.
export const iso = (ms) => new Date(ms).toISOString();
/// EVERY SPEED THE SITE STATES is over this many whole hours: one hour swings
/// five-fold between two quiet ones, a stopped hour says "never", and who is
/// online follows the clock round the world, so a window under a day reads the
/// time of day. A pace, an hours-to-go, a per-hour count: all of them.
export const PACE_HOURS = 24;
export const msOf = (t) => (t === null || t === undefined ? t : typeof t === "number" ? t : Date.parse(t));
