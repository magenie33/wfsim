function libraryQueryNode(path) {
  return path === "" ? finder.query.where : path.split(".").reduce((n, index) => n.children[Number(index)], finder.query.where);
}
function libraryQueryOptions(items, value) {
  return items.map(([id, label]) => `<option value="${escHtml(id)}"${id === value ? " selected" : ""}>${escHtml(tr(label))}</option>`).join("");
}
function libraryAttributeFilters(all) {
  if (!finder.advanced) return "";
  const query = finder.query, w = weaponInfo(finder.weapon);
  const values = (field) => field === "equipment" ? [...new Set(all.flatMap(finderTokens))].sort().map((v) => [v, finderTokenName(v)])
    : field === "mode" ? w.modes.map((v) => [v, modeLabel(w, v)])
      : field === "riven" ? [["with", tr("With riven")], ["without", tr("Without riven")]]
        : (META.buff_triggers || []).map((t) => [t.id, tr(BUFF_TRIGGER_NAME[t.id] || t.id)]);
  const control = (path, field, choices, value) => `<select data-query-${field}="${path}" aria-label="${escHtml(tr(field === "field" ? "Attribute or equipment" : "Comparison"))}">${libraryQueryOptions(choices, value)}</select>`;
  const node = (n, path, depth) => {
    if (n.children) return `<div class="library-query-group"><div class="library-query-group-head"><b>(</b>`
      + control(path, "logic", [["and", "AND · all conditions"], ["or", "OR · any condition"]], n.op)
      + (path ? `<button type="button" data-query-remove="${path}" aria-label="${escHtml(tr("remove"))}">×</button>` : "") + `</div>`
      + n.children.map((c, i) => node(c, path ? path + "." + i : String(i), depth + 1)).join("")
      + `<div class="library-query-add"><button type="button" class="ghost-btn small" data-query-add="${path}"${libraryQueryLeaves().length >= 32 ? " disabled" : ""}>＋ ${escHtml(tr("Add condition"))}</button>`
      + `<button type="button" class="ghost-btn small" data-query-group="${path}"${depth >= 5 ? " disabled" : ""}>＋ ${escHtml(tr("Add group"))}</button><b>)</b></div></div>`;
    const numeric = LIBRARY_NUMERIC_FIELDS.some(([id]) => id === n.field), unit = libraryStaticField(n.field)?.[2] || "";
    const value = numeric ? `<label class="library-query-number"><input type="number" min="0" step="any" data-query-value="${path}" value="${n.value ?? ""}" aria-label="${escHtml(tr("Value"))}"><span>${escHtml(unit)}</span></label>`
      : ddButton("library-query-value-" + path.replaceAll(".", "-"), { value: n.value || "", search: true,
        items: values(n.field).map(([value, label]) => ({ value, label })), onPick: (value) => { libraryQueryNode(path).value = value; libraryCommitQuery(); } });
    return `<div class="library-query-rule">${control(path, "field", LIBRARY_QUERY_FIELDS, n.field)}`
      + control(path, "cmp", (numeric ? [">=", ">", "<=", "<", "=", "!="] : ["=", "!="]).map((v) => [v, v]), n.cmp)
      + value + `<button type="button" data-query-remove="${path}" aria-label="${escHtml(tr("remove"))}">×</button></div>`;
  };
  const where = query.where.children ? query.where : { op: "and", children: [query.where] };
  if (where !== query.where) query.where = where;
  return `<section class="library-advanced"><p>${escHtml(tr("Attribute comparisons include permanent bonuses only."))}</p>${node(where, "", 0)}`
    + libraryExpressionEditor("filter") + `</section>`;
}
function libraryExpressionEditor(area) {
  const sorting = area === "sort", id = sorting ? "library-sort" : "library-query";
  const value = sorting ? finder.sortDraft ?? librarySortLanguage() : finder.queryDraft ?? libraryExpressionText(finder.query.where);
  return `<label class="library-query-language">${escHtml(tr(sorting ? "Sort expression" : "Filter expression"))}<textarea id="${id}-text" spellcheck="false">${escHtml(value)}</textarea></label>`
    + `<div class="library-query-actions"><button type="button" class="ghost-btn small" data-query-apply="${area}">${escHtml(tr("Apply expression"))}</button>`
    + `<span id="${id}-error" role="status">${escHtml(sorting ? finder.sortError || "" : finder.queryError || "")}</span></div>`;
}
const librarySortLanguage = () => "ORDER BY " + finder.query.orderBy.map((o) => o.field + " " + o.direction.toUpperCase()).join(", ");
const librarySortLabel = () => finder.query.orderBy.map((o) => tr(LIBRARY_ORDER_FIELDS.find(([id]) => id === o.field)?.[1] || o.field)
  + (o.direction === "asc" ? " ↑" : " ↓")).join(" → ");
function libraryRetainedSort() {
  return `<div class="library-retained-sort"><span>${escHtml(tr("Advanced sort retained"))}: ${escHtml(librarySortLabel())}</span>`
    + `<button type="button" class="ghost-btn small" data-library-sort-edit>${escHtml(tr("Edit"))}</button></div>`;
}
function libraryRetainedFilters() {
  if (finder.advanced || !libraryComplexTerms().some((n) => libraryQueryLeaves(n).some((c) => c.value != null))) return "";
  return `<div class="library-retained-filter"><span>${escHtml(tr("Some active conditions cannot be shown with simple controls."))}</span>`
    + `<button type="button" class="ghost-btn small" data-library-filter-edit>${escHtml(tr("Edit advanced conditions"))}</button></div>`;
}
function librarySortPanel() {
  const options = [["score", "Highest score"], ["score-asc", "Lowest score"], ["recent", "Recently calculated"], ["drain", "Lowest base MOD drain"]];
  if (finder.sort === "custom") options.push(["custom", "Custom sort"]);
  const heading = `<div class="library-panel-heading"><h2 id="library-sort-title">${escHtml(tr("Sort"))}</h2><div class="library-panel-actions library-sort-control">`
    + (finder.advancedSort ? "" : `<div class="library-sort">${ddButton("library-sort-picker", { value: finder.sort, title: tr("Sort"), data: { "library-sort": "" },
      items: options.map(([value, label]) => ({ value, label: tr(label) })) })}</div>`)
    + `<button type="button" class="ghost-btn small" data-library-sort-advanced aria-pressed="${finder.advancedSort}">${escHtml(tr(finder.advancedSort ? "Simple sort" : "Advanced sort"))}</button></div></div>`;
  const start = `<div class="library-panel library-sort-panel" id="library-sort" aria-labelledby="library-sort-title">${heading}`;
  if (!finder.advancedSort) return start + (finder.sort === "custom" ? libraryRetainedSort() : "") + `</div>`;
  return start + `<div class="library-sort-editor"><h3>${escHtml(tr("Sort priority"))}</h3><div class="library-query-order">`
    + finder.query.orderBy.map((o, i) => `<div class="library-query-order-row"><span>${i + 1}</span>`
      + ddButton("library-sort-field-" + i, { value: o.field, title: tr("Sort"), data: { "query-sort-field": i },
        items: LIBRARY_ORDER_FIELDS.filter(([field]) => field === o.field || !finder.query.orderBy.some((x) => x.field === field)).map(([value, label]) => ({ value, label: tr(label) })) })
      + ddButton("library-sort-direction-" + i, { value: o.direction, title: tr("Sort direction"), data: { "query-sort-direction": i },
        items: [["desc", "Descending"], ["asc", "Ascending"]].map(([value, label]) => ({ value, label: tr(label) })) })
      + `<button type="button" data-query-sort-up="${i}"${i ? "" : " disabled"} aria-label="${escHtml(tr("Move up"))}">↑</button>`
      + `<button type="button" data-query-sort-remove="${i}"${finder.query.orderBy.length === 1 ? " disabled" : ""} aria-label="${escHtml(tr("remove"))}">×</button></div>`).join("")
    + `<button type="button" class="ghost-btn small" data-query-sort-add${finder.query.orderBy.length >= 8 ? " disabled" : ""}>＋ ${escHtml(tr("Add sort"))}</button></div>`
    + libraryExpressionEditor("sort") + `</div></div>`;
}

function libraryAttributeChanged(el) {
  if (el.matches("[data-query-sort-field], [data-query-sort-direction]")) {
    const field = el.hasAttribute("data-query-sort-field");
    finder.query.orderBy[Number(field ? el.dataset.querySortField : el.dataset.querySortDirection)][field ? "field" : "direction"] = el.value;
  } else if (el.matches("[data-query-logic], [data-query-field], [data-query-cmp], [data-query-value]")) {
    const kind = ["logic", "field", "cmp", "value"].find((k) => el.hasAttribute("data-query-" + k));
    const n = libraryQueryNode(el.getAttribute("data-query-" + kind));
    if (kind === "field") Object.assign(n, { field: el.value, cmp: LIBRARY_NUMERIC_FIELDS.some(([id]) => id === el.value) ? ">=" : "=", value: null });
    else if (kind === "value") n.value = el.value === "" ? null : Math.max(0, el.valueAsNumber);
    else n[kind === "logic" ? "op" : kind] = el.value;
  } else return false;
  libraryProjectQuery();
  if (el.matches("[data-query-sort-field], [data-query-sort-direction]")) { finder.sortDraft = null; finder.sortError = ""; }
  else { finder.queryDraft = null; finder.queryError = ""; }
  return true;
}
function libraryQueryAction(el) {
  const action = el.closest("[data-query-add], [data-query-group], [data-query-remove], [data-query-apply], [data-query-sort-add], [data-query-sort-remove], [data-query-sort-up]");
  if (!action) return false;
  const d = action.dataset;
  if ("queryApply" in d) {
    const sorting = d.queryApply === "sort", id = sorting ? "library-sort" : "library-query";
    const text = $(id + "-text").value;
    try {
      const parsed = libraryParseQuery(text, sorting ? { sortOnly: true } : { filterOnly: true });
      if (sorting) finder.query.orderBy = parsed.orderBy; else finder.query.where = parsed.where;
    } catch (e) {
      if (sorting) { finder.sortDraft = text; finder.sortError = e.message; }
      else { finder.queryDraft = text; finder.queryError = e.message; }
      $(id + "-error").textContent = e.message; return true;
    }
  } else if ("queryAdd" in d || "queryGroup" in d) {
    const group = "queryGroup" in d, parent = libraryQueryNode(group ? d.queryGroup : d.queryAdd);
    parent.children.push(group ? { op: "or", children: [] } : { field: "crit_chance", cmp: ">=", value: null });
  } else if ("queryRemove" in d) {
    const path = d.queryRemove.split("."), at = Number(path.pop()); libraryQueryNode(path.join(".")).children.splice(at, 1);
  } else if ("querySortAdd" in d) {
    const field = LIBRARY_ORDER_FIELDS.find(([id]) => !finder.query.orderBy.some((o) => o.field === id))?.[0];
    if (field) finder.query.orderBy.push({ field, direction: "desc" });
  } else if ("querySortRemove" in d) finder.query.orderBy.splice(Number(d.querySortRemove), 1);
  else if ("querySortUp" in d) {
    const i = Number(d.querySortUp), rows = finder.query.orderBy; [rows[i - 1], rows[i]] = [rows[i], rows[i - 1]];
  }
  libraryCommitQuery(d.queryApply === "sort" || Object.keys(d).some((k) => k.startsWith("querySort")) ? "sort" : "filter"); return true;
}
