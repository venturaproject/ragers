import { useEffect, useRef, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { AuthenticatedLayout } from '@/layouts'
import { Main } from '@/components/layout'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Textarea } from '@/components/ui/textarea'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { Input } from '@/components/ui/input'
import { cn } from '@/lib/utils'
import {
  Filter,
  Loader2,
  MessageCircleQuestion,
  MoreHorizontal,
  Pencil,
  Plus,
  Send,
  Sparkles,
  Trash2,
} from 'lucide-react'
import { ragApi, type QuerySource, type RagDocument } from '@/services/rag-api'
import { chatApi, type ChatMessage, type ChatThread } from '@/services/chat-api'
import { toast } from 'sonner'

type ApiErrorLike = { response?: { status?: number; data?: { error?: string } } }

function friendlyError(error: unknown, fallback: string): string {
  const err = error as ApiErrorLike
  if (err?.response?.status === 503) {
    return 'La IA no está configurada en este servidor (falta LLM_API_KEY).'
  }
  return err?.response?.data?.error ?? fallback
}

function SourcesList({ sources }: { sources: QuerySource[] }) {
  const [open, setOpen] = useState(false)
  if (sources.length === 0) return null
  return (
    <div className="mt-2 border-t border-border/60 pt-2">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="text-xs font-medium text-muted-foreground underline underline-offset-2"
      >
        {open ? 'Ocultar' : 'Ver'} {sources.length} fuente(s)
      </button>
      {open && (
        <div className="mt-2 space-y-2">
          {sources.map((s, i) => (
            <div
              key={`${s.document_id}-${s.chunk_index}`}
              className="rounded-md border bg-background/50 p-2 text-xs"
            >
              <div className="mb-1 flex items-center gap-2 text-muted-foreground">
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
    </div>
  )
}

/**
 * Chat RAG — persisted threads around POST /api/rag/query. Threads are
 * created lazily on the first message of a new conversation. The backend
 * has no conversation memory: each question is still answered independently
 * against the index — threads are a record of what was asked, not context
 * fed back into the model.
 */
export default function AskPage() {
  const [input, setInput] = useState('')
  const [activeThreadId, setActiveThreadId] = useState<string | null>(null)
  const [draftMessages, setDraftMessages] = useState<ChatMessage[]>([])
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const [topK, setTopK] = useState<number | undefined>(undefined)
  const [showFilter, setShowFilter] = useState(false)
  const [renamingId, setRenamingId] = useState<string | null>(null)
  const [renameValue, setRenameValue] = useState('')
  const [threadToDelete, setThreadToDelete] = useState<ChatThread | null>(null)
  const scrollRef = useRef<HTMLDivElement>(null)
  const queryClient = useQueryClient()

  const threadsQuery = useQuery({
    queryKey: ['chat-threads'],
    queryFn: () => chatApi.listThreads(),
  })

  const threadQuery = useQuery({
    queryKey: ['chat-thread', activeThreadId],
    queryFn: () => chatApi.getThread(activeThreadId as string),
    enabled: !!activeThreadId,
  })

  const docsQuery = useQuery({
    queryKey: ['rag-documents'],
    queryFn: () => ragApi.listDocuments(),
  })
  const readyDocs: RagDocument[] = (docsQuery.data ?? []).filter((d) => d.status === 'ready')

  // Messages shown: the persisted thread's once loaded, else the local
  // draft accumulated for a not-yet-created (or still loading) thread.
  const messages: ChatMessage[] = activeThreadId ? (threadQuery.data?.messages ?? []) : draftMessages

  const deleteThread = useMutation({
    mutationFn: (id: string) => chatApi.deleteThread(id),
    onSuccess: (_void, id) => {
      queryClient.invalidateQueries({ queryKey: ['chat-threads'] })
      if (activeThreadId === id) {
        setActiveThreadId(null)
        setDraftMessages([])
      }
      toast.success('Conversación eliminada')
    },
    onError: () => toast.error('No se pudo eliminar la conversación.'),
  })

  const renameThread = useMutation({
    mutationFn: ({ id, title }: { id: string; title: string }) =>
      chatApi.renameThread(id, title),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['chat-threads'] })
      setRenamingId(null)
    },
    onError: () => toast.error('No se pudo cambiar el nombre.'),
  })

  const ask = useMutation({
    mutationFn: async (question: string) => {
      let threadId = activeThreadId
      if (!threadId) {
        const title = question.length > 60 ? `${question.slice(0, 60)}…` : question
        const thread = await chatApi.createThread(title)
        threadId = thread.id
        setActiveThreadId(threadId)
      }
      const result = await chatApi.ask(threadId, question, [...selected], topK)
      return { threadId, result }
    },
  })

  // Once there's something to show (a selected thread, a draft message, or
  // a request in flight), the layout switches from the centered "new chat"
  // input to the bottom-anchored one — same idea as ChatGPT's home screen.
  const hasContent = !!activeThreadId || messages.length > 0 || ask.isPending

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: 'smooth' })
  }, [messages.length, ask.isPending])

  const toggle = (id: string) => {
    setSelected((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  const send = () => {
    const question = input.trim()
    if (!question || ask.isPending) return
    setInput('')
    const optimisticUser: ChatMessage = {
      id: crypto.randomUUID(),
      role: 'user',
      content: question,
      sources: null,
      created_at: new Date().toISOString(),
    }
    setDraftMessages((prev) => [...prev, optimisticUser])

    ask.mutate(question, {
      onSuccess: ({ threadId, result }) => {
        queryClient.invalidateQueries({ queryKey: ['chat-threads'] })
        queryClient.invalidateQueries({ queryKey: ['chat-thread', threadId] })
        setDraftMessages([])
      },
      onError: (error) => {
        setDraftMessages((prev) => [
          ...prev,
          {
            id: crypto.randomUUID(),
            role: 'assistant',
            content: friendlyError(error, 'No se pudo responder la pregunta.'),
            sources: null,
            created_at: new Date().toISOString(),
          },
        ])
      },
    })
  }

  const newConversation = () => {
    setActiveThreadId(null)
    setDraftMessages([])
  }

  return (
    <AuthenticatedLayout title="Chat RAG">
      <Main fixed>
        <section className="flex h-full gap-4">
          {/* Thread list */}
          <div className="flex w-60 flex-none flex-col gap-2 sm:w-64">
            <Button variant="outline" size="sm" className="gap-2" onClick={newConversation}>
              <Plus className="h-4 w-4" />
              Nueva conversación
            </Button>
            <div className="flex-1 space-y-1 overflow-y-auto rounded-lg border p-2">
              {threadsQuery.isLoading && (
                <p className="p-2 text-xs text-muted-foreground">Cargando…</p>
              )}
              {threadsQuery.data?.length === 0 && (
                <p className="p-2 text-xs text-muted-foreground">
                  Sin conversaciones todavía.
                </p>
              )}
              {threadsQuery.data?.map((t) => (
                <div
                  key={t.id}
                  className={cn(
                    'group flex items-center gap-1 rounded-md px-2 py-1.5 text-sm hover:bg-muted',
                    activeThreadId === t.id && 'bg-muted font-medium',
                  )}
                >
                  {renamingId === t.id ? (
                    <Input
                      autoFocus
                      value={renameValue}
                      onChange={(e) => setRenameValue(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter') {
                          e.preventDefault()
                          const title = renameValue.trim()
                          if (title) renameThread.mutate({ id: t.id, title })
                          else setRenamingId(null)
                        }
                        if (e.key === 'Escape') setRenamingId(null)
                      }}
                      onBlur={() => {
                        const title = renameValue.trim()
                        if (title && title !== t.title) renameThread.mutate({ id: t.id, title })
                        else setRenamingId(null)
                      }}
                      className="h-7 flex-1 px-1.5 text-sm"
                    />
                  ) : (
                    <button
                      type="button"
                      onClick={() => {
                        setDraftMessages([])
                        setActiveThreadId(t.id)
                      }}
                      className="min-w-0 flex-1 truncate text-left"
                      title={t.title}
                    >
                      {t.title}
                    </button>
                  )}

                  {renamingId !== t.id && (
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <button
                          type="button"
                          className="shrink-0 rounded text-muted-foreground opacity-0 hover:bg-background hover:text-foreground group-hover:opacity-100 data-[state=open]:opacity-100"
                          aria-label="Acciones de la conversación"
                          onClick={(e) => e.stopPropagation()}
                        >
                          <MoreHorizontal className="h-4 w-4" />
                        </button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end">
                        <DropdownMenuItem
                          onClick={() => {
                            setRenamingId(t.id)
                            setRenameValue(t.title)
                          }}
                        >
                          <Pencil className="mr-2 h-4 w-4" />
                          Cambiar el nombre
                        </DropdownMenuItem>
                        <DropdownMenuItem
                          className="text-destructive focus:text-destructive"
                          onClick={() => setThreadToDelete(t)}
                        >
                          <Trash2 className="mr-2 h-4 w-4" />
                          Eliminar
                        </DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  )}
                </div>
              ))}
            </div>
          </div>

          {/* Chat panel */}
          <div className="flex h-full flex-1 flex-col gap-4">
            <div className="flex flex-none items-start justify-between gap-4">
              <div>
                <div className="flex items-center gap-2">
                  <MessageCircleQuestion className="h-6 w-6 text-primary" />
                  <h2 className="text-2xl font-bold tracking-tight">Chat RAG</h2>
                </div>
                <p className="mt-1 text-muted-foreground">
                  Pregunta sobre todo lo indexado: PDFs subidos y datos sincronizados desde el
                  ERP/CRM — ambos aparecen juntos en{' '}
                  <a href="/admin/rag/documents" className="underline underline-offset-2">
                    Documentos
                  </a>
                  .
                </p>
              </div>
              {readyDocs.length > 0 && (
                <Button
                  variant="outline"
                  size="sm"
                  className="gap-2"
                  onClick={() => setShowFilter((v) => !v)}
                >
                  <Filter className="h-4 w-4" />
                  Filtrar
                  {selected.size > 0 && <Badge variant="secondary">{selected.size}</Badge>}
                </Button>
              )}
            </div>

            {showFilter && (
              <div className="flex flex-none flex-col gap-3 rounded-lg border bg-muted/30 p-3">
                <div className="flex items-center gap-2">
                  <p className="text-xs font-medium text-muted-foreground">
                    Fragmentos a recuperar (top_k)
                  </p>
                  <Select
                    value={topK ? String(topK) : 'default'}
                    onValueChange={(v) => setTopK(v === 'default' ? undefined : Number(v))}
                  >
                    <SelectTrigger className="h-8 w-32">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="default">Por defecto</SelectItem>
                      <SelectItem value="5">5</SelectItem>
                      <SelectItem value="8">8</SelectItem>
                      <SelectItem value="12">12</SelectItem>
                      <SelectItem value="20">20</SelectItem>
                    </SelectContent>
                  </Select>
                </div>

                {readyDocs.length > 0 && (
                  <div className="flex flex-wrap gap-3">
                    <p className="w-full text-xs font-medium text-muted-foreground">
                      Buscar solo en (vacío busca en todo lo indexado)
                    </p>
                    {readyDocs.map((d) => (
                      <label key={d.id} className="flex items-center gap-2 text-sm">
                        <Checkbox
                          checked={selected.has(d.id)}
                          onCheckedChange={() => toggle(d.id)}
                        />
                        <span className="max-w-[200px] truncate">{d.filename}</span>
                      </label>
                    ))}
                  </div>
                )}
              </div>
            )}

            {hasContent ? (
              <>
                <div
                  ref={scrollRef}
                  className="flex-1 overflow-y-auto rounded-lg border bg-background/50 p-4"
                >
                  <div className="mx-auto flex w-full max-w-[60%] flex-col gap-4">
                    {threadQuery.isLoading && activeThreadId ? (
                      <div className="flex h-40 items-center justify-center text-muted-foreground">
                        <Loader2 className="h-6 w-6 animate-spin" />
                      </div>
                    ) : (
                      <>
                        {messages.map((m) => (
                          <div
                            key={m.id}
                            className={cn(
                              'flex',
                              m.role === 'user' ? 'justify-end' : 'justify-start',
                            )}
                          >
                            <div
                              className={cn(
                                'max-w-[85%] rounded-2xl px-4 py-2.5 text-sm shadow-sm',
                                m.role === 'user'
                                  ? 'rounded-br-sm bg-primary text-primary-foreground'
                                  : 'rounded-bl-sm border bg-secondary/60',
                              )}
                            >
                              <p className="whitespace-pre-wrap">{m.content}</p>
                              {m.sources && m.sources.length > 0 && (
                                <SourcesList sources={m.sources} />
                              )}
                            </div>
                          </div>
                        ))}

                        {ask.isPending && (
                          <div className="flex justify-start">
                            <div className="flex items-center gap-2 rounded-2xl rounded-bl-sm border bg-secondary/60 px-4 py-2.5 text-sm text-muted-foreground shadow-sm">
                              <Loader2 className="h-4 w-4 animate-spin" /> Pensando…
                            </div>
                          </div>
                        )}
                      </>
                    )}
                  </div>
                </div>

                <div className="mx-auto w-full max-w-[60%] flex-none">
                  <ChatInputBar
                    input={input}
                    setInput={setInput}
                    onSend={send}
                    disabled={ask.isPending}
                  />
                </div>
              </>
            ) : (
              <div className="flex flex-1 flex-col items-center justify-center gap-6">
                <div className="flex flex-col items-center gap-2 text-center text-muted-foreground">
                  <Sparkles className="h-10 w-10" />
                  <p className="text-lg">¿Qué quieres saber?</p>
                </div>
                <div className="w-full max-w-[60%]">
                  <ChatInputBar
                    input={input}
                    setInput={setInput}
                    onSend={send}
                    disabled={ask.isPending}
                  />
                </div>
              </div>
            )}
          </div>
        </section>
      </Main>

      <AlertDialog open={!!threadToDelete} onOpenChange={(open) => !open && setThreadToDelete(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>¿Eliminar esta conversación?</AlertDialogTitle>
            <AlertDialogDescription>
              Se borrará "{threadToDelete?.title}" y todos sus mensajes. Esta acción no se puede
              deshacer.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancelar</AlertDialogCancel>
            <AlertDialogAction
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
              onClick={() => {
                if (threadToDelete) deleteThread.mutate(threadToDelete.id)
                setThreadToDelete(null)
              }}
            >
              Eliminar
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </AuthenticatedLayout>
  )
}

function ChatInputBar({
  input,
  setInput,
  onSend,
  disabled,
}: {
  input: string
  setInput: (v: string) => void
  onSend: () => void
  disabled: boolean
}) {
  return (
    <div className="flex w-full items-end gap-2 rounded-3xl border bg-background px-4 py-2.5 shadow-sm focus-within:ring-1 focus-within:ring-ring">
      <Textarea
        rows={1}
        placeholder="¿Qué quieres saber?"
        value={input}
        onChange={(e) => setInput(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault()
            onSend()
          }
        }}
        disabled={disabled}
        className="max-h-40 min-h-[28px] resize-none border-0 bg-transparent p-0 shadow-none focus-visible:ring-0"
      />
      <Button
        onClick={onSend}
        disabled={!input.trim() || disabled}
        size="icon"
        className="h-9 w-9 shrink-0 rounded-full"
      >
        {disabled ? <Loader2 className="h-4 w-4 animate-spin" /> : <Send className="h-4 w-4" />}
      </Button>
    </div>
  )
}
