-- Documents ingested into the RAG index, and the chunks they were split
-- into. Embedding vectors are not stored here — they live in Qdrant, keyed
-- by chunk id; these tables are metadata (+ the chunk text, so the admin
-- panel and any future re-embedding don't need a round-trip to Qdrant).

CREATE TABLE IF NOT EXISTS documents (
    id           UUID PRIMARY KEY,
    owner        TEXT,
    filename     TEXT        NOT NULL,
    status       TEXT        NOT NULL,
    pages        INTEGER     NOT NULL DEFAULT 0,
    chunk_count  INTEGER     NOT NULL DEFAULT 0,
    error        TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS documents_owner_idx ON documents(owner);
CREATE INDEX IF NOT EXISTS documents_created_at_idx ON documents(created_at DESC);

CREATE TABLE IF NOT EXISTS chunks (
    id          UUID PRIMARY KEY,
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    idx         INTEGER NOT NULL,
    text        TEXT NOT NULL,
    char_start  BIGINT NOT NULL,
    char_end    BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS chunks_document_id_idx ON chunks(document_id);
