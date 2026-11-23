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

CREATE TABLE IF NOT EXISTS record_compute_order_stock_sources (
  stock text COLLATE "C" NOT NULL,
  first_ordinal bigint NOT NULL CHECK (first_ordinal > 0),
  file text COLLATE "C" NOT NULL,
  import_id text COLLATE "C" NOT NULL UNIQUE REFERENCES state_compute_order_imports(id),
  PRIMARY KEY (stock,first_ordinal),
  UNIQUE (stock,file)
);
REVOKE ALL ON record_compute_order_stock_sources FROM PUBLIC;

CREATE TABLE IF NOT EXISTS state_compute_order_stock_window (
  slot boolean PRIMARY KEY DEFAULT true CHECK (slot),
  key text COLLATE "C" NOT NULL UNIQUE,
  import_id text COLLATE "C" NOT NULL REFERENCES state_compute_order_imports(id),
  first_byte bigint NOT NULL CHECK (first_byte>=0),
  next_byte bigint NOT NULL CHECK (next_byte>first_byte),
  max_open bigint NOT NULL CHECK (max_open>0),
  positions jsonb NOT NULL
);
CREATE TABLE IF NOT EXISTS state_compute_order_stock_pages (
  window_key text COLLATE "C" NOT NULL REFERENCES state_compute_order_stock_window(key) ON DELETE CASCADE,
  first_byte bigint NOT NULL CHECK (first_byte>=0),
  next_byte bigint NOT NULL CHECK (next_byte>first_byte),
  orders integer NOT NULL CHECK (orders>0),
  PRIMARY KEY (window_key,first_byte)
);
REVOKE ALL ON state_compute_order_stock_window,state_compute_order_stock_pages FROM PUBLIC;
