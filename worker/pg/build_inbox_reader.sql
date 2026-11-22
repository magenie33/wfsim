CREATE OR REPLACE FUNCTION build_inbox_scan_ids(text,bigint) RETURNS TABLE(id text)
LANGUAGE sql STABLE SECURITY DEFINER ROWS 100 SET search_path FROM CURRENT AS $$
  WITH bounds AS (SELECT greatest(0,least(coalesce($2,0),100)) AS rows), candidates AS (
    SELECT legacy.id FROM (
      SELECT b.id FROM record_build_submissions b WHERE b.id>$1 COLLATE "C"
      ORDER BY b.id LIMIT (SELECT rows FROM bounds)
    ) legacy
    UNION ALL
    SELECT active.id FROM state_build_inbox_generations g CROSS JOIN LATERAL (
      SELECT p.id FROM state_build_inbox_positions p
      WHERE p.generation=g.generation AND p.generation IS NOT NULL AND p.id>$1 COLLATE "C"
        AND EXISTS(SELECT FROM record_build_inbox_payloads b
          WHERE (b.generation,b.id)=(p.payload_generation,p.id) AND b.state='pending' OFFSET 0)
      ORDER BY p.id LIMIT (SELECT rows FROM bounds)
    ) active
    UNION ALL
    SELECT equipment.id FROM (
      SELECT b.id FROM record_equipment_submissions b WHERE b.id>$1 COLLATE "C"
      ORDER BY b.id LIMIT (SELECT rows FROM bounds)
    ) equipment
  )
  SELECT c.id FROM candidates c ORDER BY c.id COLLATE "C" LIMIT (SELECT rows FROM bounds)
$$;
REVOKE ALL ON FUNCTION build_inbox_scan_ids(text,bigint) FROM PUBLIC;
