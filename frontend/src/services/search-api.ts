import { axios } from '@/lib/axios'
import { endpoints } from '@/lib/endpoints'

export interface SearchHit {
  document_id: string
  chunk_index: number
  text: string
  score: number
}

export const searchApi = {
  /**
   * GET /api/search — BM25 keyword search over ingested chunks. No LLM
   * call, works even without LLM_API_KEY configured.
   */
  search(q: string, limit = 10): Promise<SearchHit[]> {
    if (!q.trim()) return Promise.resolve([])
    return axios
      .get<{ hits: SearchHit[] }>(endpoints.search, { params: { q, limit } })
      .then((r) => r.data.hits)
  },
}
