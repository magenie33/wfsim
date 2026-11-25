-- Apply once, after accounts.sql. The GitHub subject is the public repository
-- owner's immutable numeric id; an operator-confirmed account id is the only fallback.
BEGIN;
DO $$
DECLARE
  recipient text;
  linked text;
  confirmed text := nullif(current_setting('wfsim.honors_recipient',true),'');
BEGIN
  SELECT account INTO linked FROM relation_account_login_identities
    WHERE provider='github' AND subject='54000348';
  IF confirmed IS NOT NULL THEN
    SELECT id INTO STRICT recipient FROM entity_accounts WHERE id=confirmed;
    IF linked IS NOT NULL AND linked<>recipient THEN
      RAISE EXCEPTION 'confirmed recipient differs from linked GitHub account';
    END IF;
  ELSE
    IF linked IS NULL THEN RAISE EXCEPTION 'simfleet developer has no linked GitHub account'; END IF;
    recipient := linked;
  END IF;
  INSERT INTO relation_account_developer_honors(account,honor)
    VALUES (recipient,'core_developer'),(recipient,'community_developer')
    ON CONFLICT DO NOTHING;
  INSERT INTO relation_community_project_developers(project,account)
    VALUES ('simfleet',recipient) ON CONFLICT (project) DO NOTHING;
  IF NOT EXISTS (SELECT 1 FROM relation_community_project_developers
    WHERE project='simfleet' AND account=recipient) THEN
    RAISE EXCEPTION 'simfleet already has a different registered developer';
  END IF;
END $$;
COMMIT;
