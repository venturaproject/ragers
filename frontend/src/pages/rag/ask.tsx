import { useState } from 'react'
import { useMutation, useQuery } from '@tanstack/react-query'
import { AuthenticatedLayout } from '@/layouts'
import { Main } from '@/components/layout'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Textarea } from '@/components/ui/textarea'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import { AlertCircle, Loader2, MessageCircleQuestion, Sparkles } from 'lucide-react'
import { ragApi, type RagDocument } from '@/services/rag-api'

type ApiErrorLike = { response?: { status?: number; data?: { error?: string } } }

function friendlyError(error: unknown, fallback: string): string {
  const err = error as ApiErrorLike
  if (err?.response?.status === 503) {
    return 'La IA no está configurada en este servidor (falta LLM_API_KEY).'
  }
  return err?.response?.data?.error ?? fallback
}

/**
 * Ask a question, answered strictly from whatever's been ingested in
 * "Documentos" — the flip side of that page. Real activity is still tracked
 * in Procesamientos.
 */
export default function AskPage() {
  const [question, setQuestion] = useState('')
  const [selected, setSelected] = useState<Set<string>>(new Set())

  const docsQuery = useQuery({
    queryKey: ['rag-documents'],
    queryFn: () => ragApi.listDocuments(),
  })
  const readyDocs: RagDocument[] = (docsQuery.data ?? []).filter((d) => d.status === 'ready')

  const ask = useMutation({
    mutationFn: () => ragApi.query(question, [...selected]),
  })

  const toggle = (id: string) => {
    setSelected((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  return (
    <AuthenticatedLayout title="Preguntar">
      <Main>
        <div className="grid max-w-5xl flex-1 items-start gap-6 md:gap-8">
          <div>
            <div className="flex items-center gap-2">
              <MessageCircleQuestion className="h-6 w-6 text-primary" />
              <h2 className="text-2xl font-bold tracking-tight">Preguntar</h2>
            </div>
            <p className="mt-1 text-muted-foreground">
              Pregunta sobre lo que hayas indexado en{' '}
              <a href="/admin/rag/documents" className="underline underline-offset-2">
                Documentos
              </a>
              . La respuesta se construye solo con lo recuperado — si no hay nada relevante, lo
              dice en vez de inventar.
            </p>
          </div>

          <Card>
            <CardContent className="space-y-4 pt-6">
              <Textarea
                rows={3}
                placeholder="¿Qué quieres saber?"
                value={question}
                onChange={(e) => setQuestion(e.target.value)}
                disabled={ask.isPending}
              />

              {readyDocs.length > 0 && (
                <div className="space-y-2">
                  <p className="text-sm font-medium">
                    Buscar solo en (opcional — vacío busca en todo lo indexado)
                  </p>
                  <div className="flex flex-wrap gap-3">
                    {readyDocs.map((d) => (
                      <label key={d.id} className="flex items-center gap-2 text-sm">
                        <Checkbox
                          checked={selected.has(d.id)}
                          onCheckedChange={() => toggle(d.id)}
                          disabled={ask.isPending}
                        />
                        <span className="max-w-[200px] truncate">{d.filename}</span>
                      </label>
                    ))}
                  </div>
                </div>
              )}

              <Button
                onClick={() => question.trim() && ask.mutate()}
                disabled={!question.trim() || ask.isPending}
                className="gap-2"
              >
                {ask.isPending ? (
                  <><Loader2 className="h-4 w-4 animate-spin" /> Pensando…</>
                ) : (
                  <><Sparkles className="h-4 w-4" /> Preguntar</>
                )}
              </Button>
            </CardContent>
          </Card>

          {ask.isError && (
            <div className="flex items-start gap-2 rounded-lg border border-red-200 bg-red-50 p-4 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/30 dark:text-red-400">
              <AlertCircle className="mt-0.5 h-4 w-4 shrink-0" />
              <span>{friendlyError(ask.error, 'No se pudo responder la pregunta.')}</span>
            </div>
          )}

          {ask.data && (
            <Card>
              <CardHeader>
                <CardTitle className="text-base">Respuesta</CardTitle>
                {ask.data.sources.length > 0 && (
                  <CardDescription>
                    <Badge variant="outline">{ask.data.sources.length} fuentes</Badge>
                  </CardDescription>
                )}
              </CardHeader>
              <CardContent className="space-y-4">
                <p className="whitespace-pre-wrap text-sm">{ask.data.answer}</p>

                {ask.data.sources.length > 0 && (
                  <div className="space-y-2 border-t pt-4">
                    <p className="text-sm font-medium">Fuentes</p>
                    {ask.data.sources.map((s, i) => (
                      <div key={`${s.document_id}-${s.chunk_index}`} className="rounded-md border p-3 text-sm">
                        <div className="mb-1 flex items-center gap-2 text-xs text-muted-foreground">
                          <Badge variant="outline">[{i + 1}]</Badge>
                          <span className="font-mono">
                            doc {s.document_id.slice(0, 8)} · chunk {s.chunk_index}
                          </span>
                          <span className="ml-auto tabular-nums">score {s.score.toFixed(3)}</span>
                        </div>
                        <p className="text-muted-foreground">{s.text}</p>
                      </div>
                    ))}
                  </div>
                )}
              </CardContent>
            </Card>
          )}
        </div>
      </Main>
    </AuthenticatedLayout>
  )
}
