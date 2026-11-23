CREATE TABLE IF NOT EXISTS state_compute_order_imports (
  id text COLLATE "C" PRIMARY KEY,
  input_sha text NOT NULL CHECK (input_sha ~ '^[0-9a-f]{64}$'),
  input_bytes bigint NOT NULL CHECK (input_bytes >= 0),
  engine text NOT NULL,
  cut text NOT NULL,
  target bigint NOT NULL CHECK (target > 0),
  next_byte bigint NOT NULL DEFAULT 0 CHECK (next_byte BETWEEN 0 AND input_bytes),
  processed bigint NOT NULL DEFAULT 0 CHECK (processed >= 0),
  admitted bigint NOT NULL DEFAULT 0 CHECK (admitted BETWEEN 0 AND target),
  paused boolean NOT NULL DEFAULT false
);

CREATE TABLE IF NOT EXISTS record_compute_order_import_batches (
  import_id text COLLATE "C" NOT NULL REFERENCES state_compute_order_imports(id),
  first_byte bigint NOT NULL CHECK (first_byte >= 0),
  next_byte bigint NOT NULL CHECK (next_byte > first_byte),
  orders integer NOT NULL CHECK (orders > 0),
  task_ids bigint[] NOT NULL,
  PRIMARY KEY (import_id, first_byte)
);
REVOKE ALL ON state_compute_order_imports,record_compute_order_import_batches FROM PUBLIC;
