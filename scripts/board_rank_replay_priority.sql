-- SPDX-License-Identifier: AGPL-3.0-or-later
-- Operator-only scheduling repair for one pinned replay. Caller supplies
-- pg_temp.rank_replay_scope(weapon,ruler,source,native,cut,revision,zone,zone_position,queued_at).
WITH target AS MATERIALIZED (
  SELECT p.weapon,p.ruler,s.zone,coalesce(s.zone_position,0) AS position,s.queued_at
  FROM pg_temp.rank_replay_scope a
  JOIN state_board_rank_partitions p ON (p.weapon,p.ruler,p.source,p.native,p.cut,p.revision)=
   (a.weapon,a.ruler,a.source,a.native,a.cut,a.revision)
  JOIN state_board_rank_new_scopes s ON (s.weapon,s.ruler)=(p.weapon,p.ruler)
  WHERE p.catalog_phase='replay' AND p.seeded AND NOT p.complete
    AND s.zone=a.zone AND s.zone_position IS NOT DISTINCT FROM a.zone_position AND s.queued_at=a.queued_at
  FOR SHARE OF p,s
), batch AS MATERIALIZED (
  SELECT d.weapon,d.ruler,d.identity,d.version,t.zone,t.position,t.queued_at
  FROM target t
  JOIN state_board_rank_baseline_obligations o ON (o.weapon,o.ruler)=(t.weapon,t.ruler)
  JOIN state_board_rank_dirty d ON (d.weapon,d.ruler,d.identity)=(o.weapon,o.ruler,o.identity)
  WHERE d.version=o.version AND d.applied_version<o.version
    AND (d.lease IS NULL OR d.lease_until<=board_input_timestamp(clock_timestamp()))
    AND (d.zone,coalesce(d.zone_position,0),coalesce(d.queued_at,d.available_at))>
        (t.zone,t.position,t.queued_at)
  ORDER BY d.identity LIMIT 32 FOR UPDATE OF d SKIP LOCKED
)
UPDATE state_board_rank_dirty d SET zone=b.zone,
  zone_position=b.position,queued_at=least(coalesce(d.queued_at,d.available_at),b.queued_at)
FROM batch b WHERE (d.weapon,d.ruler,d.identity,d.version)=(b.weapon,b.ruler,b.identity,b.version)
RETURNING d.weapon,d.ruler,d.identity,d.version,d.zone,d.zone_position;
