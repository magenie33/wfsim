-- Apply once, after accounts.sql. The GitHub subject is the public repository
-- owner's immutable numeric id; never grant against a mutable display name.
BEGIN;
DO $$
DECLARE recipient text;
BEGIN
  SELECT account INTO STRICT recipient FROM relation_account_login_identities
    WHERE provider='github' AND subject='54000348';
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
