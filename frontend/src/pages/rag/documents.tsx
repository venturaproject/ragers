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
  Database,
  FileText,
  Loader2,
  RefreshCw,
  Search,
  ShieldCheck,
  Trash2,
  Upload,
  XCircle,
} from 'lucide-react'
import { FileDropzone } from '@/components/file-dropzone'
import { Checkbox } from '@/components/ui/checkbox'
import { Input } from '@/components/ui/input'
import { ragApi, type DocumentStatus, type RagDocument } from '@/services/rag-api'
import { searchApi } from '@/services/search-api'
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
  const [redactPii, setRedactPii] = useState(false)
  const [redactPiiErp, setRedactPiiErp] = useState(false)
  const [searchQuery, setSearchQuery] = useState('')
  const [activeSearch, setActiveSearch] = useState('')
  const queryClient = useQueryClient()

  const listQuery = useQuery({
    queryKey: ['rag-documents'],
    queryFn: () => ragApi.listDocuments(),
    refetchInterval: (query) =>
      query.state.data?.some((d) => IN_FLIGHT.includes(d.status)) ? 2000 : false,
  })

  const ingest = useMutation({
    mutationFn: () => ragApi.ingest(files[0] as File, redactPii),
    onSuccess: (doc) => {
      setFiles([])
      queryClient.invalidateQueries({ queryKey: ['rag-documents'] })
      if (doc.pii_redacted) {
        toast.success(`Documento indexado — ${doc.pii_redacted} dato(s) de PII redactados`)
      }
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

  const syncErp = useMutation({
    mutationFn: () => ragApi.syncErp(redactPiiErp),
    onSuccess: (result) => {
      queryClient.invalidateQueries({ queryKey: ['rag-documents'] })
      const piiSuffix = result.pii_redacted > 0 ? `, ${result.pii_redacted} dato(s) de PII redactados` : ''
      if (result.failed > 0) {
        toast.warning(`ERP sincronizado: ${result.ingested} ok, ${result.failed} fallidos${piiSuffix}`)
      } else {
        toast.success(`ERP sincronizado: ${result.ingested} documento(s) indexados${piiSuffix}`)
      }
    },
    onError: (error) => {
      const err = error as { response?: { status?: number; data?: { error?: string } } }
      toast.error(
        err?.response?.status === 503
          ? 'El ERP/CRM externo no está configurado en este servidor (falta ERP_DATABASE_URL).'
          : (err?.response?.data?.error ?? 'No se pudo sincronizar con el ERP.'),
      )
    },
  })

  const docs: RagDocument[] = listQuery.data ?? []
  const docsById = new Map(docs.map((d) => [d.id, d]))

  const searchResults = useQuery({
    queryKey: ['search', activeSearch],
    queryFn: () => searchApi.search(activeSearch),
    enabled: activeSearch.length > 0,
  })

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
              <a href="/admin/rag/ask" className="underline underline-offset-2">Chat RAG</a>.
            </p>
          </div>

          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2 text-base">
                <Search className="h-4 w-4 text-primary" />
                Búsqueda por palabra clave (BM25)
              </CardTitle>
              <CardDescription>
                Coincidencia exacta sobre el texto ya indexado — no pasa por el LLM, funciona
                aunque no haya <code>LLM_API_KEY</code>. Para respuestas con lenguaje natural usá{' '}
                <a href="/admin/rag/ask" className="underline underline-offset-2">Chat RAG</a>.
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-3">
              <div className="flex gap-2">
                <Input
                  placeholder="ej: OFC-1001, Mayor S.A., pedido #3…"
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  onKeyDown={(e) => e.key === 'Enter' && setActiveSearch(searchQuery.trim())}
                />
                <Button
                  variant="outline"
                  onClick={() => setActiveSearch(searchQuery.trim())}
                  disabled={!searchQuery.trim()}
                >
                  Buscar
                </Button>
              </div>
              {searchResults.isFetching && (
                <p className="text-xs text-muted-foreground">Buscando…</p>
              )}
              {searchResults.data && (
                <div className="space-y-2">
                  <p className="text-xs text-muted-foreground">
                    {searchResults.data.length} resultado(s)
                  </p>
                  {searchResults.data.map((hit) => (
                    <div
                      key={`${hit.document_id}-${hit.chunk_index}`}
                      className="rounded-md border p-3 text-sm"
                    >
                      <div className="mb-1 flex items-center gap-2 text-xs text-muted-foreground">
                        <span className="font-medium text-foreground">
                          {docsById.get(hit.document_id)?.filename ?? hit.document_id.slice(0, 8)}
                        </span>
                        <span>chunk {hit.chunk_index}</span>
                        <span className="ml-auto tabular-nums">score {hit.score.toFixed(2)}</span>
                      </div>
                      <p className="line-clamp-3 text-muted-foreground">{hit.text}</p>
                    </div>
                  ))}
                </div>
              )}
            </CardContent>
          </Card>

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
              <label className="flex items-start gap-2 text-sm text-muted-foreground">
                <Checkbox
                  checked={redactPii}
                  onCheckedChange={(v) => setRedactPii(v === true)}
                  disabled={ingest.isPending}
                  className="mt-0.5"
                />
                <span>
                  <span className="flex items-center gap-1.5 font-medium text-foreground">
                    <ShieldCheck className="h-3.5 w-3.5" /> Redactar PII antes de indexar
                  </span>
                  Quita emails, DNI/NIE, IBAN, tarjetas y teléfonos reconocidos antes de que el
                  texto salga a NVIDIA — best-effort, no una garantía de compliance. Si después
                  necesitás preguntar por esos datos exactos, la respuesta ya no va a estar.
                </span>
              </label>
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
              <CardTitle className="flex items-center gap-2 text-base">
                <Database className="h-4 w-4 text-primary" />
                Fuente externa: ERP/CRM
              </CardTitle>
              <CardDescription>
                Trae productos y pedidos desde la base de datos del ERP/CRM (un sistema que esta
                API no posee) y los indexa igual que un PDF. Cada sincronización reingesta todo
                como documentos nuevos — pensado para una sync puntual o periódica, no para
                ejecutarse a alta frecuencia.
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <label className="flex items-start gap-2 text-sm text-muted-foreground">
                <Checkbox
                  checked={redactPiiErp}
                  onCheckedChange={(v) => setRedactPiiErp(v === true)}
                  disabled={syncErp.isPending}
                  className="mt-0.5"
                />
                <span>
                  <span className="flex items-center gap-1.5 font-medium text-foreground">
                    <ShieldCheck className="h-3.5 w-3.5" /> Redactar PII antes de indexar
                  </span>
                  Misma redacción best-effort aplicada a cada producto/pedido antes de embeder.
                </span>
              </label>
              <Button
                variant="outline"
                onClick={() => syncErp.mutate()}
                disabled={syncErp.isPending}
                className="gap-2"
              >
                {syncErp.isPending ? (
                  <><Loader2 className="h-4 w-4 animate-spin" /> Sincronizando…</>
                ) : (
                  <><RefreshCw className="h-4 w-4" /> Sincronizar ERP</>
                )}
              </Button>
            </CardContent>
          </Card>

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
