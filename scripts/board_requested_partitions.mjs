// SPDX-License-Identifier: AGPL-3.0-or-later
// Only freshly captured requested targets may ask the reader to prepare a new partition.
export async function enqueueRequestedPartition(sql,source,rulers) {
 const [{result}]=await sql.unsafe('SELECT board_requested_partition_enqueue($1::text,$2::uuid) result',
  [JSON.stringify(rulers.map(({id,scene})=>({id,scene}))),source]);
 return result;
}
