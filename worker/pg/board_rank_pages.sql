-- Both storage layouts expose only complete, immutable snapshot revisions.
CREATE OR REPLACE FUNCTION board_read_page(text,text,text,bigint,bigint) RETURNS jsonb
LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE held record; requested text; pages jsonb; last_ordinal bigint; generation uuid;
BEGIN
 IF $4 IS NULL OR $4<0 OR $5 IS NULL OR $5<1 OR $5>128 THEN RAISE EXCEPTION 'invalid board row page';END IF;
 SELECT revision INTO requested FROM state_board_public_heads WHERE weapon=$1 AND ruler=$2;
 IF requested IS NULL THEN RETURN jsonb_build_object('ok',false,'error','not_ready');END IF;
 requested:=coalesce($3,requested);
 SELECT s.* INTO held FROM record_board_read_snapshots s WHERE (s.weapon,s.ruler,s.revision)=($1,$2,requested)
  AND NOT EXISTS(SELECT 1 FROM state_board_public_withdrawn_revisions w
   WHERE (w.weapon,w.ruler,w.revision)=(s.weapon,s.ruler,s.revision));
 IF NOT FOUND THEN RETURN jsonb_build_object('ok',false,'error','revision_unavailable');END IF;
 SELECT token INTO generation FROM record_board_rank_snapshot_refs WHERE weapon=$1 AND ruler=$2 AND revision=requested;
 IF generation IS NULL THEN
  SELECT coalesce(jsonb_agg(body::jsonb ORDER BY ordinal),'[]'),coalesce(max(ordinal),$4) INTO pages,last_ordinal
  FROM(SELECT ordinal,body FROM state_board_read_rows WHERE weapon=$1 AND ruler=$2 AND revision=requested
   AND ordinal>$4 ORDER BY ordinal LIMIT $5)x;
 ELSE
  SELECT coalesce(jsonb_agg(body::jsonb ORDER BY ordinal),'[]'),coalesce(max(ordinal),$4) INTO pages,last_ordinal
  FROM(SELECT ordinal,body FROM state_board_rank_snapshot_rows WHERE token=generation
   AND ordinal>$4 ORDER BY ordinal LIMIT $5)x;
 END IF;
 RETURN jsonb_build_object('ok',true,'weapon',$1,'ruler',$2,'revision',requested,'source',held.source,
  'engine',held.engine,'captured_at',held.captured_at,'count',held.rows,'after',$4,'rows',pages,
  'next',CASE WHEN last_ordinal<held.rows THEN last_ordinal END);
END $$;
REVOKE ALL ON FUNCTION board_read_page(text,text,text,bigint,bigint) FROM PUBLIC;
