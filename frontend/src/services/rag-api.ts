import { axios } from '@/lib/axios'
import { endpoints } from '@/lib/endpoints'

// The shared axios instance defaults to `application/json`; for multipart we
// must clear it so the browser sets `multipart/form-data; boundary=…` itself.
const MULTIPART = { headers: { 'Content-Type': null } }

export type DocumentStatus = 'pending' | 'chunking' | 'embedding' | 'ready' | 'failed'

export interface RagChunk {
  index: number
  text: string
  char_start: number
  char_end: number
}

export interface RagDocument {
  id: string
  filename: string
  status: DocumentStatus
  pages: number
  chunk_count: number
  error: string | null
  created_at: string
  updated_at: string
  /** Only present right after ingest: how many PII matches were redacted. */
  pii_redacted?: number
}

export interface RagDocumentDetail extends RagDocument {
  chunks: RagChunk[]
}

export interface AiUsage {
  prompt_tokens: number
  completion_tokens: number
  total_tokens: number
}

export interface QuerySource {
  document_id: string
  chunk_index: number
  text: string
  score: number
}

export interface QueryResult {
  answer: string
  sources: QuerySource[]
}

export interface ErpSyncItem {
  title: string
  status: 'ready' | 'failed'
  error: string | null
}

export interface ErpSyncResult {
  ingested: number
  failed: number
  pii_redacted: number
  items: ErpSyncItem[]
}

export const ragApi = {
  /**
   * POST /api/rag/documents — ingest a PDF: extract, chunk, embed, index.
   * `redactPii`: strip recognised PII (emails, IDs, cards, phone numbers)
   * before the text is chunked/embedded — off by default, since it's lossy
   * (a redacted value can no longer be asked about later).
   */
  ingest(file: File, redactPii?: boolean): Promise<RagDocument> {
    const form = new FormData()
    form.append('file', file)
    if (redactPii) form.append('redact_pii', 'true')
    return axios
      .post<RagDocument>(endpoints.rag.documents, form, MULTIPART)
      .then((r) => r.data)
  },

  listDocuments(): Promise<RagDocument[]> {
    return axios.get<RagDocument[]>(endpoints.rag.documents).then((r) => r.data)
  },

  getDocument(id: string): Promise<RagDocumentDetail> {
    return axios.get<RagDocumentDetail>(endpoints.rag.documentDetail(id)).then((r) => r.data)
  },

  deleteDocument(id: string): Promise<void> {
    return axios.delete(endpoints.rag.documentDetail(id)).then(() => undefined)
  },

  /** POST /api/rag/query — ask a question, answered strictly from what's indexed. */
  query(question: string, documentIds?: string[], topK?: number): Promise<QueryResult> {
    return axios
      .post<QueryResult>(endpoints.rag.query, {
        question,
        document_ids: documentIds?.length ? documentIds : undefined,
        top_k: topK,
      })
      .then((r) => r.data)
  },

  /**
   * POST /api/rag/sync/erp — pull products + orders from the external
   * ERP/CRM database and ingest each as a document, same pipeline as a PDF.
   */
  syncErp(redactPii?: boolean): Promise<ErpSyncResult> {
    return axios
      .post<ErpSyncResult>(endpoints.rag.syncErp, undefined, {
        params: redactPii ? { redact_pii: true } : undefined,
      })
      .then((r) => r.data)
  },
}
