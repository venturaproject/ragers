import { axios } from '@/lib/axios'
import { endpoints } from '@/lib/endpoints'
import type { QuerySource } from '@/services/rag-api'

export interface ChatThread {
  id: string
  title: string
  created_at: string
  updated_at: string
}

export interface ChatMessage {
  id: string
  role: 'user' | 'assistant'
  content: string
  sources: QuerySource[] | null
  created_at: string
}

export interface ChatThreadDetail extends ChatThread {
  messages: ChatMessage[]
}

export interface AskResult {
  answer: string
  sources: QuerySource[]
}

export const chatApi = {
  /** POST /api/chat/threads — start a new persisted thread. */
  createThread(title?: string): Promise<ChatThread> {
    return axios
      .post<ChatThread>(endpoints.chat.threads, { title })
      .then((r) => r.data)
  },

  /** GET /api/chat/threads — this caller's threads, most recent first. */
  listThreads(): Promise<ChatThread[]> {
    return axios.get<ChatThread[]>(endpoints.chat.threads).then((r) => r.data)
  },

  /** GET /api/chat/threads/:id — thread + its messages. */
  getThread(id: string): Promise<ChatThreadDetail> {
    return axios.get<ChatThreadDetail>(endpoints.chat.threadDetail(id)).then((r) => r.data)
  },

  deleteThread(id: string): Promise<void> {
    return axios.delete(endpoints.chat.threadDetail(id)).then(() => undefined)
  },

  /** PATCH /api/chat/threads/:id — rename a thread. */
  renameThread(id: string, title: string): Promise<ChatThread> {
    return axios
      .patch<ChatThread>(endpoints.chat.threadDetail(id), { title })
      .then((r) => r.data)
  },

  /**
   * POST /api/chat/threads/:id/messages — ask a question inside a thread.
   * Persisted on the server either way; the RAG call itself is still
   * stateless per-question (no conversation memory in the model).
   */
  ask(threadId: string, question: string, documentIds?: string[], topK?: number): Promise<AskResult> {
    return axios
      .post<AskResult>(endpoints.chat.messages(threadId), {
        question,
        document_ids: documentIds?.length ? documentIds : undefined,
        top_k: topK,
      })
      .then((r) => r.data)
  },
}
