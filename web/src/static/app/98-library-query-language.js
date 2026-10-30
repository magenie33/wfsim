// A bounded expression grammar; text is parsed as data and is never evaluated as code.
function libraryExpressionText(n, inner = false) {
  if (n.children) {
    const parts = n.children.map((c) => libraryExpressionText(c, true)).filter(Boolean);
    const text = parts.join(n.op === "and" ? " AND " : " OR ");
    return inner && parts.length > 1 ? "(" + text + ")" : text;
  }
  if (n.value == null || n.value === "") return "";
  return `${n.field} ${n.cmp} ${typeof n.value === "number" ? n.value : JSON.stringify(n.value)}`;
}
function libraryQueryLanguage(q) {
  const where = libraryExpressionText(q.where);
  const defaultOrder = q.orderBy.length === 1 && q.orderBy[0].field === "score" && q.orderBy[0].direction === "desc";
  return where + (defaultOrder ? "" : (where ? " " : "") + "ORDER BY " + q.orderBy.map((o) => o.field + " " + o.direction.toUpperCase()).join(", "));
}
function libraryParseQuery(text, options = {}) {
  if (text.length > 8000) throw new Error(tr("Query is too long."));
  const tokens = [];
  const pattern = /\s+|"(?:[^"\\]|\\.)*"|>=|<=|!=|&&|\|\||[()=><,]|(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?|[A-Za-z_][A-Za-z_0-9]*/gy;
  let at = 0;
  while (at < text.length) {
    pattern.lastIndex = at; const m = pattern.exec(text);
    if (!m) throw new Error(trF("Invalid query near position {n}.", { n: at + 1 }));
    at = pattern.lastIndex;
    if (!/^\s+$/.test(m[0])) tokens.push(m[0]);
  }
  let i = 0, leaves = 0;
  const peek = () => (tokens[i] || "").toUpperCase();
  const take = () => tokens[i++];
  const need = (value) => { if (peek() !== value) throw new Error(trF("Expected {token}.", { token: value })); take(); };
  const group = (op, left, right) => left.op === op ? { op, children: [...left.children, right] } : { op, children: [left, right] };
  const primary = (depth) => {
    if (depth > 6 || ++leaves > 64) throw new Error(tr("Query is too complex."));
    if (peek() === "(") { take(); const n = or(depth + 1); need(")"); return n; }
    const field = take();
    if (!LIBRARY_QUERY_FIELDS.some(([id]) => id === field)) throw new Error(trF("Unknown filter field: {field}.", { field: field || "—" }));
    const cmp = take();
    if (!["=", "!=", ">", "<", ">=", "<="].includes(cmp)) throw new Error(tr("Expected a comparison operator."));
    const numeric = LIBRARY_NUMERIC_FIELDS.some(([id]) => id === field);
    const raw = take(); let value;
    if (numeric) {
      value = raw == null || raw.startsWith('"') ? NaN : Number(raw);
      if (!Number.isFinite(value) || value < 0) throw new Error(tr("Expected a non-negative number."));
    } else {
      if (!["=", "!="].includes(cmp) || !raw?.startsWith('"')) throw new Error(tr("Use = or != with a quoted value."));
      try { value = JSON.parse(raw); } catch (_) { throw new Error(tr("Invalid quoted value.")); }
      if (!value || value.length > 200) throw new Error(tr("Invalid quoted value."));
      if (field === "riven" && !["with", "without"].includes(value)) throw new Error(tr("Riven must be with or without."));
    }
    return { field, cmp, value };
  };
  const and = (depth) => { let n = primary(depth); while (["AND", "&&"].includes(peek())) { take(); n = group("and", n, primary(depth)); } return n; };
  const or = (depth) => { let n = and(depth); while (["OR", "||"].includes(peek())) { take(); n = group("or", n, and(depth)); } return n; };
  if (options.sortOnly && tokens.length && peek() !== "ORDER") throw new Error(tr("Edit filters in the filter area."));
  let where = { op: "and", children: [] };
  if (tokens.length && peek() !== "ORDER") where = or(0);
  if (!where.children) where = { op: "and", children: [where] };
  let orderBy = [{ field: "score", direction: "desc" }];
  if (peek() === "ORDER") {
    if (options.filterOnly) throw new Error(tr("Edit sorting in the sort area."));
    take(); need("BY"); orderBy = [];
    for (;;) {
      const field = take();
      if (!LIBRARY_ORDER_FIELDS.some(([id]) => id === field) || orderBy.some((o) => o.field === field)) throw new Error(tr("Unknown or repeated sort field."));
      const direction = ["ASC", "DESC"].includes(peek()) ? take().toLowerCase() : "asc";
      orderBy.push({ field, direction });
      if (orderBy.length > 8) throw new Error(tr("Query is too complex."));
      if (peek() !== ",") break;
      take();
    }
  }
  if (i !== tokens.length) throw new Error(trF("Unexpected token: {token}.", { token: tokens[i] }));
  return { where, orderBy };
}
