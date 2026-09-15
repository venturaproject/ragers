import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { AuthenticatedLayout } from '@/layouts'
import { Main } from '@/components/layout'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import {
  AlertCircle,
  CheckCircle2,
  CircleDashed,
  FileText,
  Loader2,
  Trash2,
  Upload,
  XCircle,
} from 'lucide-react'
import { FileDropzone } from '@/components/file-dropzone'
import { ragApi, type DocumentStatus, type RagDocument } from '@/services/rag-api'
import { toast } from 'sonner'

const STATUS_META: Record<
  DocumentStatus,
  { label: string; icon: typeof CircleDashed; className: string; spin?: boolean }
> = {
  pending: {
    label: 'En cola',
    icon: CircleDashed,
    className: 'border-border bg-muted text-muted-foreground',
  },
  chunking: {
    label: 'Troceando',
    icon: Loader2,
    className: 'border-sky-500/25 bg-sky-500/10 text-sky-700 dark:text-sky-300',
    spin: true,
  },
  embedding: {
    label: 'Generando embeddings',
    icon: Loader2,
    className: 'border-sky-500/25 bg-sky-500/10 text-sky-700 dark:text-sky-300',
    spin: true,
  },
  ready: {
    label: 'Listo',
    icon: CheckCircle2,
    className: 'border-emerald-500/25 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300',
  },
  failed: {
    label: 'Fallido',
    icon: XCircle,
    className: 'border-destructive/25 bg-destructive/10 text-destructive',
  },
}

function DocumentStatusBadge({ status }: { status: DocumentStatus }) {
  const meta = STATUS_META[status]
  const Icon = meta.icon
  return (
    <Badge variant="outline" className={`gap-1.5 font-medium ${meta.className}`}>
      <Icon className={`h-3.5 w-3.5 ${meta.spin ? 'animate-spin' : ''}`} aria-hidden />
      {meta.label}
    </Badge>
  )
}

const IN_FLIGHT: DocumentStatus[] = ['pending', 'chunking', 'embedding']

/**
 * Ingest PDFs into the RAG index and see what's there. The real value is in
 * `POST /api/rag/query` (see "Preguntar") — this is where the corpus it
 * answers from gets built.
 */
export default function DocumentsPage() {
  const [files, setFiles] = useState<File[]>([])
  const queryClient = useQueryClient()

  const listQuery = useQuery({
    queryKey: ['rag-documents'],
    queryFn: () => ragApi.listDocuments(),
    refetchInterval: (query) =>
      query.state.data?.some((d) => IN_FLIGHT.includes(d.status)) ? 2000 : false,
  })

  const ingest = useMutation({
    mutationFn: () => ragApi.ingest(files[0] as File),
    onSuccess: () => {
      setFiles([])
      queryClient.invalidateQueries({ queryKey: ['rag-documents'] })
    },
    onError: () => toast.error('No se pudo ingerir el documento.'),
  })

  const remove = useMutation({
    mutationFn: (id: string) => ragApi.deleteDocument(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['rag-documents'] })
      toast.success('Documento eliminado')
    },
    onError: () => toast.error('No se pudo eliminar el documento.'),
  })

  const docs: RagDocument[] = listQuery.data ?? []

  return (
    <AuthenticatedLayout title="Documentos">
      <Main>
        <div className="grid max-w-5xl flex-1 items-start gap-6 md:gap-8">
          <div>
            <div className="flex items-center gap-2">
              <FileText className="h-6 w-6 text-primary" />
              <h2 className="text-2xl font-bold tracking-tight">Documentos</h2>
            </div>
            <p className="mt-1 text-muted-foreground">
              Sube un PDF para indexarlo: se extrae el texto, se trocea, se generan embeddings y
              se guarda en Qdrant. Requiere <code>LLM_API_KEY</code> configurada en el servidor.
              Luego pregúntale en{' '}
              <a href="/admin/rag/ask" className="underline underline-offset-2">Preguntar</a>.
            </p>
          </div>

          <Card>
            <CardContent className="space-y-4 pt-6">
              <FileDropzone
                value={files}
                onChange={(f) => {
                  setFiles(f)
                  ingest.reset()
                }}
                accept="application/pdf,.pdf"
                multiple={false}
                maxSizeMb={100}
                hint="PDF (con texto — no escaneado)"
                disabled={ingest.isPending}
              />
              <Button
                onClick={() => files[0] && ingest.mutate()}
                disabled={!files[0] || ingest.isPending}
                className="gap-2"
              >
                {ingest.isPending ? (
                  <><Loader2 className="h-4 w-4 animate-spin" /> Indexando…</>
                ) : (
                  <><Upload className="h-4 w-4" /> Ingerir documento</>
                )}
              </Button>
            </CardContent>
          </Card>

          {ingest.isError && (
            <div className="flex items-start gap-2 rounded-lg border border-red-200 bg-red-50 p-4 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/30 dark:text-red-400">
              <AlertCircle className="mt-0.5 h-4 w-4 shrink-0" />
              <span>
                {(ingest.error as { response?: { status?: number; data?: { error?: string } } })
                  ?.response?.status === 503
                  ? 'La IA no está configurada en este servidor (falta LLM_API_KEY).'
                  : ((ingest.error as { response?: { data?: { error?: string } } })?.response
                      ?.data?.error ?? 'No se pudo ingerir el documento.')}
              </span>
            </div>
          )}

          <Card>
            <CardHeader>
              <CardTitle className="text-base">Indexados</CardTitle>
              <CardDescription>{docs.length} documento(s)</CardDescription>
            </CardHeader>
            <CardContent className="p-0">
              <div className="overflow-x-auto">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Archivo</TableHead>
                      <TableHead>Estado</TableHead>
                      <TableHead className="text-right">Páginas</TableHead>
                      <TableHead className="text-right">Chunks</TableHead>
                      <TableHead>Creado</TableHead>
                      <TableHead />
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {docs.map((d) => (
                      <TableRow key={d.id}>
                        <TableCell className="max-w-[240px] truncate font-medium">
                          {d.filename}
                          {d.error && (
                            <span className="block text-xs font-normal text-destructive">
                              {d.error}
                            </span>
                          )}
                        </TableCell>
                        <TableCell>
                          <DocumentStatusBadge status={d.status} />
                        </TableCell>
                        <TableCell className="text-right tabular-nums">{d.pages}</TableCell>
                        <TableCell className="text-right tabular-nums">{d.chunk_count}</TableCell>
                        <TableCell className="text-xs text-muted-foreground">
                          {new Date(d.created_at).toLocaleString('es-ES')}
                        </TableCell>
                        <TableCell className="text-right">
                          <Button
                            variant="ghost"
                            size="icon"
                            className="h-8 w-8 text-muted-foreground hover:text-destructive"
                            disabled={remove.isPending}
                            onClick={() => remove.mutate(d.id)}
                          >
                            <Trash2 className="h-4 w-4" />
                          </Button>
                        </TableCell>
                      </TableRow>
                    ))}
                    {docs.length === 0 && (
                      <TableRow>
                        <TableCell colSpan={6} className="text-center text-muted-foreground">
                          {listQuery.isLoading ? 'Cargando…' : 'Sin documentos todavía'}
                        </TableCell>
                      </TableRow>
                    )}
                  </TableBody>
                </Table>
              </div>
            </CardContent>
          </Card>
        </div>
      </Main>
    </AuthenticatedLayout>
  )
}
