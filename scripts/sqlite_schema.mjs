// Schema changes are one transaction; callers open their working catalog, never a protected capture.
export const INVENTORY_TABLES={
  meta:"state_inventory_generation",records:"entity_inventory_builds",owners:"relation_inventory_owners",symbols:"entity_inventory_symbols",
  members:"relation_inventory_owner_members",questions:"state_inventory_computations",totals:"state_inventory_totals",
  jobs:"state_inventory_jobs",job_inputs:"relation_inventory_job_inputs",receipts:"record_inventory_applied_changes",epoch_tokens:"state_inventory_epoch_tokens",
  rel_sources:"entity_catalog_sources",rel_candidates:"entity_catalog_candidates",rel_facts:"state_catalog_measurements",
  rel_publication_sources:"state_catalog_publication_sources",rel_publication_facts:"state_catalog_publication_measurements",rel_publication_dirty:"state_catalog_publication_dirty",pub_files:"state_board_publication_files",pub_history:"state_board_file_history",pub_carry:"state_board_retained_rows",pub_rows:"state_board_publication_rows",pub_shapes:"state_board_publication_shapes",pub_max:"state_board_publication_maxima",pub_final:"state_board_publication_selection",pub_leaders:"state_board_publication_leaders",pub_listed:"state_board_publication_counts",
  rel_queue:"state_catalog_pending_work",rel_leaders:"state_catalog_leaders",rel_groups:"state_catalog_groups",
  rel_ranges:"state_catalog_ranges",rel_jobs:"state_catalog_jobs",rel_contract:"state_catalog_contract",
};

export function migrateSqliteTables(db,renames=INVENTORY_TABLES) {
  db.exec("BEGIN IMMEDIATE");
  try {
    const version=db.prepare("PRAGMA user_version").get().user_version;
    if(version!==0 && version!==1)throw new Error("unsupported SQLite layout version: "+version);
    const tables=new Set(db.prepare("SELECT name FROM sqlite_schema WHERE type='table'").all().map(row=>row.name));
    for(const [old,name] of Object.entries(renames)) {
      if(!/^[a-z][a-z0-9_]*$/.test(old) || !/^(entity|record|relation|state)_[a-z0-9_]+$/.test(name))throw new Error("invalid SQLite table mapping");
      if(tables.has(old) && (version!==0 || tables.has(name)))throw new Error("ambiguous SQLite migration: "+old);
    }
    for(const [old,name] of Object.entries(renames))if(tables.has(old))db.exec(`ALTER TABLE ${old} RENAME TO ${name}`);
    db.exec("PRAGMA user_version=1;COMMIT");
  } catch(error) {db.exec("ROLLBACK");throw error;}
}
