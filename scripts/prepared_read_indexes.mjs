// SPDX-License-Identifier: AGPL-3.0-or-later
// Source-owned read indexes change access cost without changing normalized values or consumer checkpoints.
export const PREPARED_EMPTY_MODES_INDEX=`CREATE INDEX prepared_board_empty_modes
 ON entity_prepared_board_versions(weapon,admit,first_cut,last_cut)
 WHERE kind='source' AND publication=1 AND body IS NOT NULL AND json_array_length(modes)=0`;

export function ensurePreparedReadIndexes(db) {
 const prior=db.prepare("SELECT sql FROM sqlite_schema WHERE name='prepared_board_empty_modes'").get();
 if(!prior)db.exec(PREPARED_EMPTY_MODES_INDEX);
 else if(prior.sql.replace(/\s+/g,' ').trim()!==PREPARED_EMPTY_MODES_INDEX.replace(/\s+/g,' ').trim())throw Error('prepared empty-mode index differs from the source-owned definition');
}
