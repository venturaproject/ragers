import { useLocation } from 'react-router-dom'
import DocumentsPage from '@/pages/rag/documents'
import AskPage from '@/pages/rag/ask'
import JobsPage from '@/pages/process/jobs'

export default function RagRoute() {
  const { pathname } = useLocation()
  if (pathname.startsWith('/admin/jobs')) return <JobsPage />
  if (pathname.startsWith('/admin/rag/ask')) return <AskPage />
  return <DocumentsPage />
}
