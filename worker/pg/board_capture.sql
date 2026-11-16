-- Only canonical board inputs feed the ranking handoff; pending computation is independent.
SELECT set_config('search_path',quote_ident(current_schema())||',pg_catalog,pg_temp',false);

CREATE OR REPLACE FUNCTION board_input_changed() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$
DECLARE scope_name text; before jsonb; after jsonb; old_key text; new_key text;
BEGIN
  IF TG_OP<>'INSERT' THEN before:=to_jsonb(OLD); END IF;
  IF TG_OP<>'DELETE' THEN after:=to_jsonb(NEW); END IF;
  CASE TG_TABLE_NAME
    WHEN 'entity_weapon_builds' THEN scope_name:='build';old_key:=before->>'id';new_key:=after->>'id';
    WHEN 'state_board_measurement_selection' THEN
      scope_name:='measurement';
      IF before IS NOT NULL THEN old_key:=jsonb_build_array(before->>'identity',before->>'ruler',before->>'mode')::text; END IF;
      IF after IS NOT NULL THEN new_key:=jsonb_build_array(after->>'identity',after->>'ruler',after->>'mode')::text; END IF;
    WHEN 'entity_measurement_reviews' THEN scope_name:='review';old_key:=before->>'id';new_key:=after->>'id';
    ELSE RAISE EXCEPTION 'unsupported board input source';
  END CASE;
  IF old_key IS NOT NULL AND old_key IS DISTINCT FROM new_key THEN PERFORM board_input_put(scope_name,old_key,NULL); END IF;
  IF new_key IS NOT NULL THEN PERFORM board_input_put(scope_name,new_key,after::text); END IF;
  RETURN NULL;
END $$;

CREATE OR REPLACE FUNCTION board_input_truncate_guard() RETURNS trigger
LANGUAGE plpgsql AS $$BEGIN
  RAISE EXCEPTION 'board input truncation requires an explicit replacement projection';
END $$;
REVOKE ALL ON FUNCTION board_input_changed(),board_input_truncate_guard() FROM PUBLIC;

DO $triggers$
DECLARE source text;
BEGIN
  FOREACH source IN ARRAY ARRAY['entity_weapon_builds','state_board_measurement_selection','entity_measurement_reviews'] LOOP
    EXECUTE format('DROP TRIGGER IF EXISTS board_input_changed ON %I',source);
    EXECUTE format('CREATE TRIGGER board_input_changed AFTER INSERT OR UPDATE OR DELETE ON %I
      FOR EACH ROW EXECUTE FUNCTION board_input_changed()',source);
    EXECUTE format('DROP TRIGGER IF EXISTS board_input_truncate ON %I',source);
    EXECUTE format('CREATE TRIGGER board_input_truncate BEFORE TRUNCATE ON %I
      FOR EACH STATEMENT EXECUTE FUNCTION board_input_truncate_guard()',source);
  END LOOP;
END $triggers$;
