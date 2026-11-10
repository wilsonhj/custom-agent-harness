-- harness-store schema, version 1 (research-2026-11-09/5-rust-core.md §4.1, without `anchors`).
-- Pragmas (WAL, synchronous=FULL, foreign_keys=ON) are set per connection in `lib.rs`.

CREATE TABLE IF NOT EXISTS schema_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);

-- Projection of each task, folded from its events; rebuildable from `events`.
CREATE TABLE IF NOT EXISTS tasks (
  id TEXT PRIMARY KEY,
  state TEXT NOT NULL,
  version INTEGER NOT NULL,
  snapshot TEXT NOT NULL,
  updated_seq INTEGER NOT NULL);

-- Receipts of accepted commands: idempotency. Same key + different body is refused.
CREATE TABLE IF NOT EXISTS commands (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL,
  idempotency_key TEXT NOT NULL,
  body_hash BLOB NOT NULL,
  body TEXT NOT NULL,
  received_at INTEGER NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('accepted','rejected')),
  rejection TEXT,
  first_seq INTEGER,
  last_seq INTEGER,
  UNIQUE (task_id, idempotency_key));

-- Append-only, hash-chained, never pruned. `body` is the canonical JSON of the whole event
-- envelope; hash = sha256(prev_hash || seq as 8 big-endian bytes || body).
CREATE TABLE IF NOT EXISTS events (
  seq INTEGER PRIMARY KEY,
  task_id TEXT NOT NULL,
  task_seq INTEGER NOT NULL,
  command_id TEXT NOT NULL REFERENCES commands(id),
  v INTEGER NOT NULL,
  type TEXT NOT NULL,
  body TEXT NOT NULL,
  at INTEGER NOT NULL,
  prev_hash BLOB NOT NULL,
  hash BLOB NOT NULL,
  UNIQUE (task_id, task_seq));
CREATE TRIGGER IF NOT EXISTS events_no_update BEFORE UPDATE ON events
  BEGIN SELECT RAISE(ABORT, 'append-only'); END;
CREATE TRIGGER IF NOT EXISTS events_no_delete BEFORE DELETE ON events
  BEGIN SELECT RAISE(ABORT, 'append-only'); END;

-- Pending and historical effects.
CREATE TABLE IF NOT EXISTS outbox (
  effect_id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  spec TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('intended','starting','started','done','abandoned')),
  attempt INTEGER NOT NULL DEFAULT 0,
  max_attempts INTEGER NOT NULL,
  process TEXT,
  intended_seq INTEGER NOT NULL,
  outcome_seq INTEGER);
CREATE INDEX IF NOT EXISTS outbox_pending ON outbox(state)
  WHERE state IN ('intended','starting','started');

-- Index of content-addressed artifacts; the bytes live under `objects/`.
CREATE TABLE IF NOT EXISTS artifacts (
  id TEXT PRIMARY KEY,
  size INTEGER NOT NULL,
  media_type TEXT NOT NULL,
  label TEXT NOT NULL,
  produced_by TEXT,
  created_seq INTEGER NOT NULL);

-- Worktree lock records. `holder_effect` NULL means released; the identity columns keep the
-- last process that ran in the worktree.
CREATE TABLE IF NOT EXISTS workspaces (
  task_id TEXT PRIMARY KEY,
  path TEXT NOT NULL UNIQUE,
  holder_effect TEXT,
  pgid INTEGER,
  leader_pid INTEGER,
  leader_start_us INTEGER,
  boot_id TEXT);
