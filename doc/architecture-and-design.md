# Architecture and design notes

This document describes how the backend is currently structured and why. It is
intended to help contributors find the right place for a change without
mistaking this pragmatic layered design for a strict implementation of every
DDD or Clean Architecture rule.

## Backend layers

The backend is organized around three cooperating areas:

| Area | Responsibility | Examples |
|---|---|---|
| `domain/` | Core entities and repository contracts, expressed as Rust traits. | `domain/rag/{entities,repository}.rs`, `domain/auth/` |
| `application/` | Use cases and orchestration. It depends on domain contracts and service clients, not on HTTP handlers. | `application/rag/service.rs`, `application/auth/service.rs` |
| `infrastructure/` | Adapters for HTTP, persistence, external services, and runtime concerns. | `infrastructure/http/`, `infrastructure/persistence/postgres/`, `application/rag/vector_store.rs` |

HTTP handlers validate/extract request data, call the relevant use case, and
translate the result to an HTTP response. Business workflows belong in
`application/`, while SQL and protocol-specific details stay in adapters.
Some integration clients (for example, Qdrant and the OpenAI-compatible LLM
client) live beside the RAG application code because they are narrow clients
used directly by those use cases; they are not domain entities.

## Repository ports and swappable persistence

Repository traits in `domain/` are ports. Both in-memory and Postgres
implementations satisfy them, and the application receives them through
`AppState` as `Arc<dyn Repository>` trait objects. `bootstrap.rs` is the
composition root: it chooses the adapters, seeds initial RBAC data, connects
optional services, and constructs the router state.

This enables the same HTTP/router integration tests to run with in-memory
repositories and without Postgres. The in-memory adapters are useful for local
development and tests; configure `DATABASE_URL` to use Postgres-backed
persistence. This is dependency inversion at the application boundary, not a
requirement that every service or external client have a generalized trait.

## RAG ingestion and answering

Both PDF uploads and ERP/CRM records converge on `ingest_text` in
`application/rag/service.rs`:

1. Extract PDF text when needed; ERP sync reads structured records and
   serializes them to text.
2. Optionally redact recognized PII before chunking or calling the model.
3. Split the text into overlapping chunks.
4. Ask the embeddings provider for passage vectors.
5. Upsert vectors and payloads into Qdrant, and persist chunk/document data via
   repositories.
6. Update the document status (`chunking` → `embedding` → `ready`), or mark it
   `failed` with an error if a step fails.

The shared pipeline is deliberate: new source types should adapt their content
to text and reuse this path instead of implementing their own chunking and
embedding behavior.

For a question, the service embeds the query, searches Qdrant with owner and
optional document filters, then sends the retrieved passages and question to
the answer provider. It returns citations from the retrieved document/chunk
identifiers. An empty retrieval produces a no-source answer rather than a
fabricated response. Authorization is enforced at the HTTP/application edge;
the owner filter at retrieval is an additional tenant-isolation boundary.

Tantivy is a separate in-process BM25 full-text search index. It supports the
search use case and is not the vector index used by RAG question answering.

## Persistence boundaries

- **Postgres** stores application records: users, sessions, roles/permissions,
  API clients/tokens, document metadata, chunks, jobs, and chat threads/messages.
- **Qdrant** stores embeddings and chunk payloads for vector retrieval. A
  document's owner and ID are included so searches can be filtered by tenant
  and selected documents.
- **Tantivy** holds an in-process BM25 index updated as documents are ingested
  and deleted. It is not a durable system of record and is not currently
  reconstructed from Postgres after a process restart.
- **ERP/CRM Postgres** is an external-source stand-in, configured separately
  from the application's own database and read by the sync use case.

Operations spanning Postgres and Qdrant are not a distributed transaction.
Code should keep failure states visible and use the document/job records to
make partial progress diagnosable; do not assume that writing one store
atomically commits the other.

## Request lifecycle and operational choices

RAG ingestion currently runs synchronously within the HTTP request. A `Job`
record provides status/diagnostics and startup reconciliation for interrupted
work, but it does **not** mean ingestion is dispatched to a background worker
or queue. If workload size or request timeouts require asynchronous processing,
that should be introduced as an explicit queue/worker design, with retry and
cross-store consistency behavior defined as part of the change.

RAG capabilities are wired together as an optional `RagEngine`. Without an LLM
API key (or when required RAG dependencies cannot be initialized), RAG routes
return `503` instead of accepting work that cannot complete. ERP sync is
similarly unavailable when its external database is not configured.

## Where to make common changes

| Change | Start here |
|---|---|
| Add an API route or change request/response behavior | `infrastructure/http/router.rs` and `infrastructure/http/handlers/` |
| Change an application workflow | `application/` |
| Add a persisted entity/repository operation | `domain/` contract, then `infrastructure/persistence/{memory,postgres}/` |
| Add a RAG source | Source adapter/use case, then reuse `application/rag/service.rs::ingest_text` |
| Change chunking, embedding, vector retrieval, or answer prompting | `application/rag/{chunker,embeddings,vector_store,answer,service}.rs` |
| Change startup wiring or optional service behavior | `bootstrap.rs`, `state.rs`, and `config.rs` |

When changing tenant-sensitive behavior, verify both authorization/scopes and
the owner/document filters passed to repositories and Qdrant. Existing router
integration tests are in `backend/tests/`; focused unit tests live alongside
the relevant application modules.
