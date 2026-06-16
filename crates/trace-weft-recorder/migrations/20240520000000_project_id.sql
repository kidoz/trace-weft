-- Tenant scoping: associate each span with the project it was ingested under.
-- Nullable so local-first single-tenant recording leaves it unset.
ALTER TABLE spans ADD COLUMN project_id TEXT;

CREATE INDEX IF NOT EXISTS idx_spans_project_id ON spans(project_id);
