// SERVER-SENT EVENTS, one `data:` payload at a time. Pure over text: the caller
// feeds what arrived and keeps the `rest` for the next chunk.

export function sseSplit(buffer) {
  const payloads = [];
  let rest = buffer, i;
  while ((i = rest.indexOf("\n")) >= 0) {
    const line = rest.slice(0, i).trim();
    rest = rest.slice(i + 1);
    if (!line.startsWith("data:")) continue;
    const data = line.slice(5).trim();
    if (!data || data === "[DONE]") continue;
    try { payloads.push(JSON.parse(data)); } catch (_) { /* a keep-alive or a partial line */ }
  }
  return { payloads, rest };
}
