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

export const ragApi = {
  /** POST /api/rag/documents — ingest a PDF: extract, chunk, embed, index. */
  ingest(file: File): Promise<RagDocument> {
    const form = new FormData()
    form.append('file', file)
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
  query(question: string, documentIds?: string[]): Promise<QueryResult> {
    return axios
      .post<QueryResult>(endpoints.rag.query, {
        question,
        document_ids: documentIds?.length ? documentIds : undefined,
      })
      .then((r) => r.data)
  },
}
