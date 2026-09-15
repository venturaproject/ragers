-- Persisted chat threads for the admin "Chat RAG" page: each thread groups a
-- back-and-forth of POST /api/rag/query calls so a caller can come back and
-- see what they asked before. Deliberately NOT fed back into the LLM prompt
-- (see application::chat::service) — this is for the caller's own record,
-- not conversation memory; each question is still answered independently.

CREATE TABLE IF NOT EXISTS chat_threads (
    id         UUID PRIMARY KEY,
    owner      TEXT,
    title      TEXT        NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS chat_threads_owner_idx ON chat_threads(owner);
CREATE INDEX IF NOT EXISTS chat_threads_updated_at_idx ON chat_threads(updated_at DESC);

CREATE TABLE IF NOT EXISTS chat_messages (
    id         UUID PRIMARY KEY,
    thread_id  UUID        NOT NULL REFERENCES chat_threads(id) ON DELETE CASCADE,
    role       TEXT        NOT NULL,
    content    TEXT        NOT NULL,
    sources    JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS chat_messages_thread_id_idx ON chat_messages(thread_id, created_at ASC);
