CREATE TABLE IF NOT EXISTS events (
    event_id TEXT NOT NULL PRIMARY KEY,
    trace_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    parent_span_id TEXT,
    seq INTEGER NOT NULL,
    event_kind TEXT NOT NULL,
    name TEXT NOT NULL,
    timestamp INTEGER NOT NULL,
    attributes TEXT NOT NULL,
    schema_version TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_events_trace_id ON events(trace_id);
CREATE INDEX IF NOT EXISTS idx_events_parent_span_id ON events(parent_span_id);
