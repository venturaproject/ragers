import { axios } from '@/lib/axios'
import { endpoints } from '@/lib/endpoints'

// ── Jobs — ingestion/query history the admin monitors ───────────────────────

export type JobStatus = 'pending' | 'running' | 'completed' | 'failed'
export type JobKind = 'sync' | 'batch'
export type JobOrigin = 'api_key' | 'oauth_client' | 'admin' | 'service'
export type Operation = 'rag_ingest' | 'rag_query'

/** Where the time went (milliseconds). See the Rust `Timings` struct. */
export interface Timings {
  open_ms: number
  read_ms: number
  convert_ms: number
  parse_ms: number
  parse_cpu_ms: number
  upload_ms: number
  total_ms: number
}

export interface FileResult {
  file: string
  /** Chunks stored (`rag_ingest`) or sources retrieved (`rag_query`). */
  rows: number
  columns: number
  elapsed_ms: number
  status: 'ok' | 'error'
  error: string | null
  timings?: Timings
  output?: string | null
}

export interface JobSummary {
  id: string
  status: JobStatus
  kind: JobKind
  operation: Operation
  origin: JobOrigin
  actor: string | null
  label: string | null
  files_total: number
  files_processed: number
  files_failed: number
  total_rows: number
  duration_ms: number | null
  created_at: string
  completed_at: string | null
  error: string | null
}

export interface Job extends JobSummary {
  results: FileResult[]
}

export interface JobStats {
  pending: number
  running: number
  completed: number
  failed: number
  avg_ms: number
}

export interface JobsPage {
  data: JobSummary[]
  current_page: number
  last_page: number
  per_page: number
  total: number
  stats: JobStats
}

export interface ListJobsParams {
  page?: number
  per_page?: number
  status?: string
  origin?: string
  operation?: string
  search?: string
}

export const jobsApi = {
  listJobs(params: ListJobsParams = {}): Promise<JobsPage> {
    return axios.get<JobsPage>(endpoints.jobs.list, { params }).then((r) => r.data)
  },

  getJob(id: string): Promise<Job> {
    return axios.get<Job>(endpoints.jobs.detail(id)).then((r) => r.data)
  },
}
