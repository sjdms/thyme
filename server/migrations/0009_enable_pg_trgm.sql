-- Enables trigram similarity search, per THYME_DATABASE_SCHEMA.md §36
-- "Patient search": "For larger deployments, PostgreSQL trigram/full-text
-- capabilities can improve search latency." Also what makes
-- `PatientRepository::search`'s `%` / `similarity()` matching (spelling
-- mishaps, transpositions, partial names) possible at all — without this
-- extension those operators don't exist.
CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE INDEX IF NOT EXISTS idx_patients_legal_name_trgm ON patients USING gin (legal_name gin_trgm_ops);

CREATE INDEX IF NOT EXISTS idx_patients_preferred_name_trgm ON patients USING gin (preferred_name gin_trgm_ops);
