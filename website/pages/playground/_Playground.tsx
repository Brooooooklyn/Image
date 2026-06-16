// website/pages/playground/_Playground.tsx
// Interactive orchestrator island: upload → controls → engine → result.
import { useCallback, useEffect, useRef, useState } from 'react'
import { PlaygroundEngine } from './_engine'
import type { RunResult } from './_engine'
import {
  Chroma,
  DISPLAYABLE,
  OUTPUT_MIME,
  ResizeFilter,
  ResizeFit,
  type CompressOp,
  type ConvertOp,
  type ResultMeta,
  type TransformOp,
} from './protocol'
import { ConvertControls, CompressControls, TransformControls } from './_controls'
import Result from './_Result'
import { showcaseRows, pct, kb } from '../_data/showcase'
import BeforeAfter from '../_components/_BeforeAfter'

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type Status = 'empty' | 'loading' | 'idle' | 'running' | 'done' | 'error'
type ActiveTab = 'convert' | 'compress' | 'transform'

// ---------------------------------------------------------------------------
// StaticFallback (Task 8)
// ---------------------------------------------------------------------------

function StaticFallback() {
  const rows = showcaseRows.slice(0, 3)
  return (
    <div data-testid="pg-status" data-status="empty" className="mx-auto max-w-4xl px-6 py-12">
      <h1 className="mb-4 text-3xl font-bold text-(--color-fg)">Playground</h1>

      <div className="mb-10 rounded-lg border border-white/10 bg-white/5 p-6">
        <p className="mb-2 text-(--color-fg)">In-browser demo unavailable</p>
        <p className="text-sm text-(--color-muted)">
          Your browser can&apos;t enable the cross-origin isolation (SharedArrayBuffer) this
          in-browser demo needs. Try Chrome or Firefox with COOP/COEP headers enabled, or run the
          demo locally.
        </p>
        <div className="mt-4 flex flex-wrap gap-3">
          <a
            href="/docs"
            className="rounded border border-white/10 bg-white/5 px-4 py-2 text-sm text-(--color-fg) hover:bg-white/10 hover:text-(--color-accent)"
          >
            Read the docs
          </a>
          <a
            href="https://github.com/Brooooooklyn/Image"
            target="_blank"
            rel="noopener noreferrer"
            className="rounded border border-white/10 bg-white/5 px-4 py-2 text-sm text-(--color-fg) hover:bg-white/10 hover:text-(--color-accent)"
          >
            GitHub
          </a>
        </div>
        <pre className="mt-4 overflow-x-auto rounded border border-white/10 bg-white/5 px-4 py-3 text-xs text-(--color-fg)">
          npm install @napi-rs/image
        </pre>
      </div>

      <p className="mb-6 text-sm text-(--color-muted)">
        Here&apos;s what @napi-rs/image produces — static showcase:
      </p>

      <div className="flex flex-col gap-10">
        {rows.map((row) => (
          <div key={row.label} className="flex flex-col gap-3">
            <div className="flex flex-wrap items-center gap-3">
              <code className="text-sm text-(--color-accent)">{row.label}</code>
              <span className="rounded border border-white/10 px-2 py-0.5 text-xs text-(--color-muted)">
                {row.kind}
              </span>
            </div>
            <BeforeAfter before={row.before} after={row.after} beforeLabel="original" afterLabel={row.label} />
            <p className="text-xs text-(--color-muted)">
              {kb(row.beforeBytes)} → {kb(row.afterBytes)}{' '}
              <span className="text-(--color-accent)">−{pct(row)}%</span>
            </p>
          </div>
        ))}
      </div>
    </div>
  )
}

// ---------------------------------------------------------------------------
// Main Playground (Task 7)
// ---------------------------------------------------------------------------

export default function Playground() {
  // ----- mount guard (avoid SSR/CSR mismatch) -----
  const [mounted, setMounted] = useState(false)
  useEffect(() => { setMounted(true) }, [])

  // ----- engine -----
  const engineRef = useRef<PlaygroundEngine | null>(null)
  useEffect(() => {
    return () => {
      engineRef.current?.dispose()
      engineRef.current = null
    }
  }, [])

  // ----- image state -----
  const originalBytesRef = useRef<ArrayBuffer | null>(null)
  const [originalUrl, setOriginalUrl] = useState<string | null>(null)
  const [meta, setMeta] = useState<ResultMeta | null>(null)

  // ----- status -----
  const [status, setStatus] = useState<Status>('empty')
  const [errorMsg, setErrorMsg] = useState<string>('')

  // ----- tab state -----
  const [activeTab, setActiveTab] = useState<ActiveTab>('convert')
  const [convertOp, setConvertOp] = useState<ConvertOp>({
    kind: 'convert',
    format: 'webp',
    quality: 75,
    chroma: Chroma.Yuv420,
  })
  const [compressOp, setCompressOp] = useState<CompressOp>({
    kind: 'compress',
    codec: 'jpeg',
    quality: 75,
    maxQuality: 80,
  })
  const [transformOp, setTransformOp] = useState<TransformOp>({
    kind: 'transform',
    resize: { enabled: false, width: 800, height: null, filter: ResizeFilter.Lanczos3, fit: ResizeFit.Cover },
    rotate: null,
    grayscale: false,
    invert: false,
    blur: null,
    encode: { format: 'webp', quality: 75 },
  })
  const [compressDisabled, setCompressDisabled] = useState(false)

  // ----- result state -----
  const resultUrlRef = useRef<string | null>(null)
  const [resultData, setResultData] = useState<{
    url: string | null
    bytes: number
    outFormat: string
  } | null>(null)
  const [resultOp, setResultOp] = useState<ConvertOp | CompressOp | TransformOp | null>(null)

  // ----- warnings -----
  const [dismissedLarge, setDismissedLarge] = useState(false)
  const [dismissedMobile, setDismissedMobile] = useState(false)

  const isLargeImage =
    meta != null &&
    originalBytesRef.current != null &&
    (meta.width * meta.height > 4_000_000 || originalBytesRef.current.byteLength > 5_000_000)

  const isMobile = mounted && typeof window !== 'undefined' && window.matchMedia('(pointer: coarse)').matches

  // ----- URL revocation helpers -----
  const revokeOriginalUrl = useCallback(() => {
    setOriginalUrl((prev) => {
      if (prev) URL.revokeObjectURL(prev)
      return null
    })
  }, [])

  const revokeResultUrl = useCallback(() => {
    if (resultUrlRef.current) {
      URL.revokeObjectURL(resultUrlRef.current)
      resultUrlRef.current = null
    }
  }, [])

  useEffect(() => {
    return () => {
      if (originalUrl) URL.revokeObjectURL(originalUrl)
      revokeResultUrl()
    }
  }, []) // eslint-disable-line react-hooks/exhaustive-deps

  // ----- image loading -----
  const processImage = useCallback(async (bytes: ArrayBuffer, blobForUrl: Blob) => {
    // Revoke previous URLs
    setOriginalUrl((prev) => {
      if (prev) URL.revokeObjectURL(prev)
      return URL.createObjectURL(blobForUrl)
    })
    revokeResultUrl()
    setResultData(null)
    setResultOp(null)
    setDismissedLarge(false)

    originalBytesRef.current = bytes
    setStatus('loading')

    // Lazily create engine
    if (!engineRef.current) {
      engineRef.current = new PlaygroundEngine()
    }

    const result: RunResult = await engineRef.current.run({ kind: 'metadata' }, bytes.slice(0))

    if (result.ok && result.kind === 'metadata') {
      setMeta(result.meta)
      setStatus('idle')
    } else if (!result.ok) {
      setStatus('error')
      setErrorMsg(result.error)
    }
  }, [revokeResultUrl])

  // ----- upload handlers -----
  const handleFiles = useCallback((files: FileList | null) => {
    if (!files || files.length === 0) return
    const file = files[0]
    const reader = new FileReader()
    reader.onload = (e) => {
      const buf = e.target?.result
      if (!(buf instanceof ArrayBuffer)) return
      const blob = new Blob([buf], { type: file.type })
      processImage(buf, blob).catch((err) => {
        setStatus('error')
        setErrorMsg(String(err))
      })
    }
    reader.readAsArrayBuffer(file)
  }, [processImage])

  const handleSample = useCallback(() => {
    fetch('/img/un-optimized.png')
      .then((r) => r.arrayBuffer())
      .then((buf) => {
        const blob = new Blob([buf], { type: 'image/png' })
        return processImage(buf, blob)
      })
      .catch((err) => {
        setStatus('error')
        setErrorMsg(String(err))
      })
  }, [processImage])

  // ----- drag-and-drop -----
  const [dragging, setDragging] = useState(false)

  const handleDrop = useCallback(
    (e: React.DragEvent<HTMLDivElement>) => {
      e.preventDefault()
      setDragging(false)
      handleFiles(e.dataTransfer.files)
    },
    [handleFiles],
  )

  // ----- file input ref -----
  const fileInputRef = useRef<HTMLInputElement>(null)

  // ----- run -----
  const activeOp = activeTab === 'convert' ? convertOp : activeTab === 'compress' ? compressOp : transformOp

  const runDisabled =
    !originalBytesRef.current ||
    status === 'loading' ||
    status === 'running' ||
    (activeTab === 'compress' && compressDisabled)

  const handleRun = useCallback(async () => {
    if (runDisabled || !originalBytesRef.current || !engineRef.current) return
    setStatus('running')
    try {
      const op = activeTab === 'convert' ? convertOp : activeTab === 'compress' ? compressOp : transformOp
      const result: RunResult = await engineRef.current.run(op, originalBytesRef.current.slice(0))

      if (result.ok && result.kind !== 'metadata') {
        revokeResultUrl()
        const mime = OUTPUT_MIME[result.outFormat]
        let url: string | null = null
        if (DISPLAYABLE(result.outFormat) && mime) {
          url = URL.createObjectURL(new Blob([result.bytes], { type: mime }))
          resultUrlRef.current = url
        }
        setResultData({ url, bytes: result.bytes.byteLength, outFormat: result.outFormat })
        setResultOp(op as ConvertOp | CompressOp | TransformOp)
        setStatus('done')
      } else if (!result.ok) {
        setStatus('error')
        setErrorMsg(result.error)
      }
    } catch (err) {
      setStatus('error')
      setErrorMsg(String(err))
    }
  }, [runDisabled, activeTab, convertOp, compressOp, transformOp, revokeResultUrl])

  // ----- SSR shell (before mount) -----
  if (!mounted) {
    return (
      <div className="mx-auto max-w-6xl px-6 py-12">
        <h1 className="mb-8 text-3xl font-bold text-(--color-fg)">Playground</h1>
        <div
          data-testid="pg-status"
          data-status="empty"
          className="flex min-h-[200px] items-center justify-center rounded-lg border border-white/10 bg-white/5 text-sm text-(--color-muted)"
        >
          Loading…
        </div>
      </div>
    )
  }

  // ----- cross-origin isolation check (only after mount) -----
  if (!self.crossOriginIsolated) {
    return <StaticFallback />
  }

  // ----- interactive UI -----
  return (
    <div className="mx-auto max-w-6xl px-6 py-12">
      <h1 className="mb-8 text-3xl font-bold text-(--color-fg)">Playground</h1>

      {/* Warnings */}
      {isLargeImage && !dismissedLarge && (
        <div className="mb-4 flex items-start justify-between rounded border border-yellow-400/20 bg-yellow-400/5 px-4 py-3 text-sm text-(--color-muted)">
          <span>Large image — encoding may be slow or memory-heavy.</span>
          <button
            onClick={() => setDismissedLarge(true)}
            className="ml-4 shrink-0 text-(--color-muted) hover:text-(--color-fg)"
            aria-label="Dismiss warning"
          >
            ×
          </button>
        </div>
      )}
      {isMobile && !dismissedMobile && (
        <div className="mb-4 flex items-start justify-between rounded border border-yellow-400/20 bg-yellow-400/5 px-4 py-3 text-sm text-(--color-muted)">
          <span>Heavy WASM on mobile may be slow or run out of memory.</span>
          <button
            onClick={() => setDismissedMobile(true)}
            className="ml-4 shrink-0 text-(--color-muted) hover:text-(--color-fg)"
            aria-label="Dismiss warning"
          >
            ×
          </button>
        </div>
      )}

      <div
        data-testid="pg-status"
        data-status={status}
        className="flex flex-col gap-8 lg:flex-row"
      >
        {/* ---- Left column: upload + controls ---- */}
        <div className="flex flex-col gap-6 lg:w-80 lg:shrink-0">
          {/* Dropzone */}
          <div
            onDrop={handleDrop}
            onDragOver={(e) => { e.preventDefault(); setDragging(true) }}
            onDragLeave={() => setDragging(false)}
            className={[
              'flex min-h-[140px] flex-col items-center justify-center gap-3 rounded-lg border-2 border-dashed p-6 text-center transition-colors',
              dragging
                ? 'border-(--color-accent) bg-white/10'
                : 'border-white/20 bg-white/5 hover:border-white/30',
            ].join(' ')}
          >
            <p className="text-sm text-(--color-muted)">Drop an image here</p>
            <div className="flex gap-2">
              <button
                onClick={() => fileInputRef.current?.click()}
                className="rounded border border-white/15 px-3 py-1.5 text-xs text-(--color-fg) hover:bg-white/10 hover:text-(--color-accent)"
              >
                Choose image
              </button>
              <button
                onClick={handleSample}
                className="rounded border border-white/15 px-3 py-1.5 text-xs text-(--color-fg) hover:bg-white/10 hover:text-(--color-accent)"
              >
                Use sample image
              </button>
            </div>
            {meta && (
              <p className="text-xs text-(--color-muted)">
                {meta.width}×{meta.height} · {meta.format.toUpperCase()}
              </p>
            )}
          </div>
          <input
            ref={fileInputRef}
            type="file"
            accept="image/*"
            className="hidden"
            onChange={(e) => handleFiles(e.target.files)}
          />

          {/* Tabs */}
          <div className="flex rounded-lg border border-white/10 bg-white/5 p-1">
            {(['convert', 'compress', 'transform'] as ActiveTab[]).map((tab) => (
              <button
                key={tab}
                role="tab"
                aria-selected={activeTab === tab}
                onClick={() => setActiveTab(tab)}
                className={[
                  'flex-1 rounded px-2 py-1.5 text-xs font-medium capitalize transition-colors',
                  activeTab === tab
                    ? 'bg-(--color-accent) text-white'
                    : 'text-(--color-muted) hover:text-(--color-fg)',
                ].join(' ')}
              >
                {tab.charAt(0).toUpperCase() + tab.slice(1)}
              </button>
            ))}
          </div>

          {/* Control panel */}
          {activeTab === 'convert' && (
            <ConvertControls value={convertOp} onChange={setConvertOp} />
          )}
          {activeTab === 'compress' && (
            <CompressControls
              value={compressOp}
              inputFormat={meta?.format ?? ''}
              onChange={setCompressOp}
              onDisabledChange={setCompressDisabled}
            />
          )}
          {activeTab === 'transform' && (
            <TransformControls value={transformOp} onChange={setTransformOp} />
          )}

          {/* Run button */}
          <button
            onClick={handleRun}
            disabled={runDisabled}
            className="w-full rounded-lg bg-(--color-accent) px-4 py-2.5 text-sm font-semibold text-white transition-opacity disabled:cursor-not-allowed disabled:opacity-40 hover:opacity-90"
          >
            Run
          </button>

          {/* Status indicators */}
          {status === 'loading' && (
            <p className="text-center text-xs text-(--color-muted)">Loading image metadata…</p>
          )}
          {status === 'running' && (
            <p className="text-center text-xs text-(--color-muted)">Processing…</p>
          )}
          {status === 'error' && (
            <p data-testid="pg-error" className="text-center text-xs text-red-400">
              Error: {errorMsg}
            </p>
          )}
        </div>

        {/* ---- Right column: preview / result ---- */}
        <div className="flex-1">
          {status === 'empty' && (
            <div className="flex min-h-[300px] items-center justify-center rounded-lg border border-white/10 bg-white/5 text-sm text-(--color-muted)">
              Upload an image to get started
            </div>
          )}
          {(status === 'idle' || status === 'loading' || status === 'running') && originalUrl && (
            <div className="flex flex-col gap-4">
              <img
                src={originalUrl}
                alt="uploaded"
                className="max-h-[400px] w-full rounded-lg border border-white/10 object-contain"
              />
              {status === 'idle' && meta && (
                <p className="text-xs text-(--color-muted)">
                  {meta.width}×{meta.height} · {meta.format.toUpperCase()} ·{' '}
                  {originalBytesRef.current
                    ? `${(originalBytesRef.current.byteLength / 1024).toFixed(1)} KB`
                    : ''}
                </p>
              )}
            </div>
          )}
          {(status === 'done' || status === 'error') &&
            resultData &&
            resultOp &&
            originalUrl &&
            originalBytesRef.current && (
              <Result
                originalUrl={originalUrl}
                originalBytes={originalBytesRef.current.byteLength}
                result={resultData}
                op={resultOp}
              />
            )}
        </div>
      </div>
    </div>
  )
}
