# Ragers

A Rust + React boilerplate for an operational, enterprise-grade RAG (Retrieval-
Augmented Generation) service: ingest PDFs and structured data from an external
system (an ERP/CRM, simulated here), index them in Qdrant, and answer questions
grounded strictly in what's indexed — with citations, multi-tenant isolation,
OAuth2 for external clients, and a persisted chat UI in the admin panel.

Forked from the sibling [Filers](../boilerplate-rust-api) boilerplate — auth,
RBAC, API clients/OAuth2, sessions, and the admin shell are shared 1:1; the
file-processing/OCR modules were dropped and a `rag` module (chunking,
embeddings, vector search, answer generation) was added in their place.

## Contents

- [Features](#features)
- [Architecture](#architecture)
- [Quick start (dev)](#quick-start-dev)
- [Configuration](#configuration)
- [RAG API](#rag-api)
- [Chat](#chat)
- [Security](#security)
- [Development](#development)
- [Production](#production)
- [Project layout](#project-layout)

## Features

- **PDF ingestion** (`POST /api/rag/documents`): extract → chunk → embed →
  store in Qdrant, with per-document status tracking (`pending` → `chunking`
  → `embedding` → `ready`/`failed`).
- **ERP/CRM sync** (`POST /api/rag/sync/erp`): pull products + orders from an
  external database this API doesn't own and ingest them through the exact
  same pipeline as a PDF — idempotent by title, so re-running a sync updates
  in place instead of piling up duplicates.
- **Grounded Q&A** (`POST /api/rag/query`): retrieval scoped by owner and
  optionally by `document_ids`, with inline `[n]` citations and an explicit
  "not in the indexed sources" answer instead of a guess.
- **Opt-in PII redaction** at ingest time (emails, IBANs, Spanish DNI/NIE,
  card numbers, phone numbers) — see [Security](#security).
- **Persisted chat threads** in the admin panel, backed by Postgres — a
  record of what was asked, not conversation memory fed back into the model.
- **Multi-tenant isolation**: OAuth2 clients only ever see their own
  documents/threads unless privileged; RBAC scopes (`rag:ingest`,
  `rag:query`) gate every route.

## Architecture

```
nginx → api (Rust/axum) ──┬─→ postgres   (users, sessions, RBAC, RAG metadata, chat threads)
        frontend (Vite)   ├─→ redis      (rate limiters, optional)
                           ├─→ qdrant     (vectors — the RAG index itself)
                           └─→ erp-db     (a *separate* Postgres — stand-in for an external
                                            ERP/CRM this API does not own)
```

`erp-db` is deliberately not the same database as `postgres`: it simulates
pulling knowledge from a system the API doesn't control, the way a real ERP/CRM
integration would look, rather than reusing the app's own persistence.

## Quick start (dev)

```bash
cp .env.example .env
# set LLM_API_KEY (NVIDIA NIM — https://build.nvidia.com/) to enable /api/rag/*
make up
```

- Admin UI: http://localhost:8085/admin (seed: `SEED_USER_EMAIL` /
  `SEED_USER_PASSWORD` in `.env`, default `admin@rag.test` / `admin1234`)
- API: http://localhost:8085/api/...
- Qdrant dashboard (optional, uncomment the port in `compose.dev.yml`):
  http://localhost:6333/dashboard

Without `LLM_API_KEY`, the server still boots — `/api/rag/*` and
`/api/chat/*/messages` answer `503` instead of half-working.

## Configuration

See `.env.example` for the full list, with rationale inline. The RAG-specific
ones:

| Variable | Default | |
|---|---|---|
| `LLM_API_KEY` | _(unset)_ | Empty ⇒ RAG routes disabled (503). One NVIDIA NIM (or other OpenAI-compatible) account behind both embeddings and chat. |
| `LLM_BASE_URL` | NVIDIA's hosted NIM catalog | **See [Security](#security) — this is the one to change for a strategic-sector deployment.** |
| `EMBEDDINGS_MODEL` | `nvidia/nemotron-3-embed-1b` | 2048-dim. Changing models means recreating the Qdrant collection. |
| `CHAT_MODEL` | `meta/llama-3.2-11b-vision-instruct` | Answer generation. |
| `QDRANT_URL` | `http://qdrant:6334` | The bundled service by default. |
| `ERP_DATABASE_URL` | _(unset)_ | Empty ⇒ `/api/rag/sync/erp` answers 503. `compose.dev.yml` bundles a seeded `erp-db` by default. |
| `RAG_TOP_K` | `8` | Default chunks retrieved per query. |
| `CHUNK_SIZE_CHARS` / `CHUNK_OVERLAP_CHARS` | `1200` / `150` | Ingestion chunking window. |

## RAG API

```bash
# Ingest a PDF (add -F redact_pii=true to strip recognised PII first)
curl -X POST http://localhost:8085/api/rag/documents \
  -H "x-api-key: $API_KEY" -F file=@invoice.pdf

# Pull the ERP/CRM's current products + orders and index them
curl -X POST "http://localhost:8085/api/rag/sync/erp?redact_pii=true" \
  -H "x-api-key: $API_KEY"

# Ask a question, grounded strictly in what's indexed
curl -X POST http://localhost:8085/api/rag/query \
  -H "x-api-key: $API_KEY" -H "Content-Type: application/json" \
  -d '{"question": "What did customer X order?", "top_k": 8}'
```

`rag:ingest` covers uploading/syncing/listing/deleting documents; `rag:query`
covers asking questions. Full spec at `/api/docs` (Scalar) or `/api/swagger`
when `ENABLE_API_DOCS` is on (default outside production).

## Chat

`/admin/rag/ask` — a ChatGPT-style UI over `POST /api/rag/query`, with threads
persisted server-side (`/api/chat/threads*`) so a caller can come back and see
what they asked. Each question is still answered **independently** — thread
history is a record for the caller, never fed back into the model's prompt, so
answers stay grounded in retrieval rather than drifting with conversation
context.

## Security

- **PII redaction, opt-in, at ingest time** (`redact_pii` on
  `POST /api/rag/documents` / `POST /api/rag/sync/erp`): pattern-matching
  (emails, IBANs, Spanish DNI/NIE, card numbers, phone numbers), applied
  *before* chunking/embedding, so recognised PII never reaches the external
  model and never lands in Qdrant or Postgres. Best-effort, not a compliance
  guarantee (see `application/rag/pii.rs`) — and deliberately off by default,
  since it's lossy: a redacted value can't be asked about afterward.
- **Prompt-injection mitigation**: the answer prompt separates `system`
  (instructions) from `user` (sources + question), with retrieved chunks
  fenced in `<source>` tags and an explicit instruction to treat their content
  as quoted data to cite — never as commands — even if a chunk's text looks
  like one. A mitigation, not a guarantee: a document engineered to hijack the
  answer is still a real risk surface for any RAG system that indexes
  content it doesn't fully trust.
- **RBAC + OAuth2 scopes** (`rag:ingest`, `rag:query`) gate every route;
  verified end-to-end (a client with the wrong scope gets `403`, and a client
  with the right scope but no documents of its own sees nothing of another
  tenant's).
- **Rate limiting** (per-IP, Redis-backed when `REDIS_URL` is set) and
  **per-client AI-token quotas** on every LLM-backed call.
- **Refuse-to-boot** in production (`APP_ENV=production`) on a weak/default
  seed password, a known default API key, or `SEED_DEMO_USERS=true`.

### Hosted vs. self-hosted inference — read this before a real deployment

By default, `LLM_BASE_URL` points at NVIDIA's free, hosted NIM catalog
(`integrate.api.nvidia.com`). That's the right choice for developing against
this boilerplate: no infrastructure to run, no cost. It is very likely **not**
the right choice for a business application in a strategic sector (defense,
critical infrastructure, health, finance) that needs the strongest realistic
data-handling guarantees: every ingested chunk and every question is sent to a
third-party cloud endpoint outside your network, with no control over data
residency and no contractual guarantee of what a free tier does with the
request.

**The fix doesn't require touching this codebase.** `LLM_BASE_URL` is already
just an OpenAI-compatible chat-completions/embeddings endpoint — NVIDIA NIM
can be **self-hosted** (the same microservices that back their cloud, running
on your own GPUs, inside your own network/VPC) behind that exact API shape.
Moving from the free hosted API to a private deployment is a one-line env
change (`LLM_BASE_URL=https://your-internal-nim/v1`), not a rewrite. The
trade-off is real, though: self-hosting means owning GPUs and the ops burden
of running them — there is no way to have "free, hosted, and data never
leaves the network" all at once. PII redaction and the prompt-injection
mitigation above reduce exposure either way, but they don't substitute for
that architectural decision when the data is genuinely sensitive.

## Development

```bash
make up          # dev stack (hot reload)
make down
make shell       # shell in the api container
make logs        # tail api logs

make test        # cargo test in the container (unit + integration)
make test-local  # cargo test on the host
make ci          # fmt --check + clippy -D warnings + test + audit
make clippy
make fmt
make audit       # cargo audit — RustSec advisory scan
```

Frontend checks (in the container):

```bash
docker compose -f compose.dev.yml exec -T frontend pnpm tsc --noEmit
docker compose -f compose.dev.yml exec -T frontend pnpm lint
```

**Tests:** unit + integration, driven through the router with
`tower::ServiceExt::oneshot` (no sockets, no real Qdrant/LLM needed — the
harness runs without `LLM_API_KEY`/`ERP_DATABASE_URL`, so RAG-dependent tests
cover auth and the "not configured" 503 path; chunking, PII redaction, and
repository logic are unit-tested directly). `backend/tests/common/mod.rs` is
the harness.

## Production

```bash
cp .env.example .env
# edit: APP_ENV=production, strong SEED_USER_PASSWORD, real API_KEYS,
#       strong POSTGRES_PASSWORD, CORS_ALLOWED_ORIGINS=https://your.domain,
#       SEED_DEMO_USERS=false, and LLM_BASE_URL if self-hosting (see Security)
make prod-up
```

`compose.prod.yml` bundles Postgres, Redis, Qdrant, and nginx (the only one
with published ports — 80/443, self-signed cert until you mount a real one).
`ERP_DATABASE_URL` is left unset by default in prod (point it at your real
ERP/CRM, or leave `/api/rag/sync/erp` disabled). To use an external managed
Postgres, Redis, or Qdrant instead, set the URL explicitly in `.env`.

## Project layout

```
backend/src/
  domain/           entities + repository traits (rag, chat, auth, rbac, api_client)
  application/       rag/ (chunker, embeddings, vector_store, answer, pii, erp_sync, service)
                      chat/ (service — ties persisted threads to rag::service)
  infrastructure/
    http/handlers/    rag.rs, chat.rs, auth.rs, api_clients.rs, ...
    persistence/       memory/ and postgres/ implementations per repository trait
frontend/src/
  pages/rag/          documents.tsx (ingest + ERP sync), ask.tsx (chat)
  services/            rag-api.ts, chat-api.ts
infrastructure/
  erp-db/init.sql      seed schema + data for the bundled ERP/CRM simulator
  nginx/, Dockerfile*   reverse proxy + build images
```
