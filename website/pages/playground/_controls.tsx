// website/pages/playground/_controls.tsx
// Presentational control panels — no wasm, no worker imports.
import { useEffect } from 'react'
import {
  Chroma,
  Orientation,
  ResizeFilter,
  type CompressCodec,
  type CompressOp,
  type ConvertFormat,
  type ConvertOp,
  type TransformOp,
} from './protocol'

// ---------------------------------------------------------------------------
// Shared primitive helpers
// ---------------------------------------------------------------------------

function Row({ children }: { children: React.ReactNode }) {
  return <div className="flex items-center gap-3">{children}</div>
}

function Label({ htmlFor, children }: { htmlFor: string; children: React.ReactNode }) {
  return (
    <label htmlFor={htmlFor} className="w-28 shrink-0 text-sm text-(--color-muted)">
      {children}
    </label>
  )
}

function Panel({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-4 rounded-lg border border-white/10 bg-white/5 p-4">
      {children}
    </div>
  )
}

function Select({
  id,
  value,
  onChange,
  children,
}: {
  id: string
  value: string | number
  onChange: (v: string) => void
  children: React.ReactNode
}) {
  return (
    <select
      id={id}
      value={value}
      onChange={(e) => onChange(e.target.value)}
      // color-scheme:dark makes the browser render the native option popup dark; without it
      // Chrome/Safari draw a light popup and the light --color-fg option text is unreadable.
      className="[color-scheme:dark] flex-1 rounded border border-white/10 bg-white/5 px-2 py-1 text-sm text-(--color-fg) focus:outline-none focus:ring-1 focus:ring-(--color-accent)"
    >
      {children}
    </select>
  )
}

function RangeWithValue({
  id,
  min,
  max,
  value,
  onChange,
}: {
  id: string
  min: number
  max: number
  value: number
  onChange: (v: number) => void
}) {
  return (
    <div className="flex flex-1 items-center gap-2">
      <input
        id={id}
        type="range"
        min={min}
        max={max}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        className="flex-1 accent-(--color-accent)"
      />
      <span className="w-8 text-right text-sm tabular-nums text-(--color-muted)">{value}</span>
    </div>
  )
}

// ---------------------------------------------------------------------------
// ConvertControls
// ---------------------------------------------------------------------------

export function ConvertControls({ value, onChange }: { value: ConvertOp; onChange: (v: ConvertOp) => void }) {
  const showQuality = value.format === 'webp' || value.format === 'avif' || value.format === 'jpeg'
  const showChroma = value.format === 'avif'

  return (
    <Panel>
      <Row>
        <Label htmlFor="cc-format">Format</Label>
        <Select
          id="cc-format"
          value={value.format}
          onChange={(v) => onChange({ ...value, format: v as ConvertFormat })}
        >
          <option value="webp">WebP</option>
          <option value="webpLossless">WebP (lossless)</option>
          <option value="avif">AVIF</option>
          <option value="jpeg">JPEG</option>
          <option value="png">PNG</option>
        </Select>
      </Row>

      {showQuality && (
        <Row>
          <Label htmlFor="cc-quality">Quality</Label>
          <RangeWithValue
            id="cc-quality"
            min={1}
            max={100}
            value={value.quality}
            onChange={(q) => onChange({ ...value, quality: q })}
          />
        </Row>
      )}

      {showChroma && (
        <Row>
          <Label htmlFor="cc-chroma">Chroma</Label>
          <Select
            id="cc-chroma"
            value={value.chroma}
            onChange={(v) => onChange({ ...value, chroma: Number(v) })}
          >
            <option value={Chroma.Yuv444}>YUV 4:4:4</option>
            <option value={Chroma.Yuv422}>YUV 4:2:2</option>
            <option value={Chroma.Yuv420}>YUV 4:2:0</option>
          </Select>
        </Row>
      )}
    </Panel>
  )
}

// ---------------------------------------------------------------------------
// CompressControls
// ---------------------------------------------------------------------------

export function CompressControls({
  value,
  inputFormat,
  onChange,
  onDisabledChange,
}: {
  value: CompressOp
  inputFormat: string
  onChange: (v: CompressOp) => void
  onDisabledChange?: (disabled: boolean) => void
}) {
  const isJpeg = inputFormat === 'jpeg'
  const isPng = inputFormat === 'png'
  const unsupported = !isJpeg && !isPng

  useEffect(() => {
    onDisabledChange?.(unsupported)
    // Normalize the codec to the input format so Run uses the right one even if the
    // user never touches the dropdown. The default CompressOp codec is 'jpeg'; without
    // this, a PNG input would run compressJpeg on PNG bytes (decode error).
    if (isPng && value.codec !== 'pngLossless' && value.codec !== 'pngQuantize') {
      onChange({ ...value, codec: 'pngLossless' })
    } else if (isJpeg && value.codec !== 'jpeg') {
      onChange({ ...value, codec: 'jpeg' })
    }
  }, [inputFormat]) // eslint-disable-line react-hooks/exhaustive-deps

  if (unsupported) {
    return (
      <Panel>
        <p className="text-sm text-(--color-muted)">
          Compress-in-place supports JPEG and PNG inputs — use Convert for other formats.
        </p>
      </Panel>
    )
  }

  if (isJpeg) {
    return (
      <Panel>
        <Row>
          <Label htmlFor="cmp-quality">Quality</Label>
          <RangeWithValue
            id="cmp-quality"
            min={1}
            max={100}
            value={value.quality}
            onChange={(q) => onChange({ ...value, codec: 'jpeg', quality: q })}
          />
        </Row>
      </Panel>
    )
  }

  // PNG
  const codec = value.codec === 'pngLossless' || value.codec === 'pngQuantize' ? value.codec : 'pngLossless'

  return (
    <Panel>
      <Row>
        <Label htmlFor="cmp-codec">Mode</Label>
        <Select
          id="cmp-codec"
          value={codec}
          onChange={(v) => onChange({ ...value, codec: v as CompressCodec })}
        >
          <option value="pngLossless">Lossless</option>
          <option value="pngQuantize">Quantize</option>
        </Select>
      </Row>

      {codec === 'pngQuantize' && (
        <Row>
          <Label htmlFor="cmp-maxquality">Max quality</Label>
          <RangeWithValue
            id="cmp-maxquality"
            min={1}
            max={100}
            value={value.maxQuality}
            onChange={(q) => onChange({ ...value, maxQuality: q })}
          />
        </Row>
      )}
    </Panel>
  )
}

// ---------------------------------------------------------------------------
// TransformControls
// ---------------------------------------------------------------------------

const ROTATE_OPTIONS: { label: string; value: string }[] = [
  { label: 'None', value: 'null' },
  { label: 'Auto (EXIF)', value: 'auto' },
  { label: '90°', value: String(Orientation.Rotate90Cw) },
  { label: '180°', value: String(Orientation.Rotate180) },
  { label: '270°', value: String(Orientation.Rotate270Cw) },
]

function rotateToString(rotate: number | 'auto' | null): string {
  if (rotate === null) return 'null'
  if (rotate === 'auto') return 'auto'
  return String(rotate)
}

function stringToRotate(v: string): number | 'auto' | null {
  if (v === 'null') return null
  if (v === 'auto') return 'auto'
  return Number(v)
}

export function TransformControls({ value, onChange }: { value: TransformOp; onChange: (v: TransformOp) => void }) {
  const blurValue = value.blur ?? 0

  return (
    <Panel>
      {/* ---- Resize ---- */}
      <fieldset className="flex flex-col gap-3">
        <legend className="mb-1 text-xs font-semibold uppercase tracking-widest text-(--color-muted)">Resize</legend>

        <Row>
          <label className="flex cursor-pointer items-center gap-2 text-sm text-(--color-fg)">
            <input
              type="checkbox"
              checked={value.resize.enabled}
              onChange={(e) => onChange({ ...value, resize: { ...value.resize, enabled: e.target.checked } })}
              className="accent-(--color-accent)"
            />
            Enable
          </label>
        </Row>

        {value.resize.enabled && (
          <>
            <Row>
              <Label htmlFor="tr-width">Width (px)</Label>
              <input
                id="tr-width"
                type="number"
                min={1}
                value={value.resize.width}
                onChange={(e) => onChange({ ...value, resize: { ...value.resize, width: Number(e.target.value) } })}
                className="flex-1 rounded border border-white/10 bg-white/5 px-2 py-1 text-sm text-(--color-fg) focus:outline-none focus:ring-1 focus:ring-(--color-accent)"
              />
            </Row>

            <Row>
              <Label htmlFor="tr-height">Height (px)</Label>
              <input
                id="tr-height"
                type="number"
                min={1}
                placeholder="auto"
                value={value.resize.height ?? ''}
                onChange={(e) =>
                  onChange({
                    ...value,
                    resize: { ...value.resize, height: e.target.value === '' ? null : Number(e.target.value) },
                  })
                }
                className="flex-1 rounded border border-white/10 bg-white/5 px-2 py-1 text-sm text-(--color-fg) placeholder-white/30 focus:outline-none focus:ring-1 focus:ring-(--color-accent)"
              />
            </Row>

            <Row>
              <Label htmlFor="tr-filter">Filter</Label>
              <Select
                id="tr-filter"
                value={value.resize.filter}
                onChange={(v) => onChange({ ...value, resize: { ...value.resize, filter: Number(v) } })}
              >
                <option value={ResizeFilter.Nearest}>Nearest</option>
                <option value={ResizeFilter.Triangle}>Triangle</option>
                <option value={ResizeFilter.CatmullRom}>CatmullRom</option>
                <option value={ResizeFilter.Gaussian}>Gaussian</option>
                <option value={ResizeFilter.Lanczos3}>Lanczos3</option>
              </Select>
            </Row>
          </>
        )}
      </fieldset>

      {/* ---- Rotate ---- */}
      <Row>
        <Label htmlFor="tr-rotate">Rotate</Label>
        <Select
          id="tr-rotate"
          value={rotateToString(value.rotate)}
          onChange={(v) => onChange({ ...value, rotate: stringToRotate(v) })}
        >
          {ROTATE_OPTIONS.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </Select>
      </Row>

      {/* ---- Adjustments ---- */}
      <fieldset className="flex flex-col gap-3">
        <legend className="mb-1 text-xs font-semibold uppercase tracking-widest text-(--color-muted)">
          Adjustments
        </legend>

        <Row>
          <label className="flex cursor-pointer items-center gap-2 text-sm text-(--color-fg)">
            <input
              type="checkbox"
              checked={value.grayscale}
              onChange={(e) => onChange({ ...value, grayscale: e.target.checked })}
              className="accent-(--color-accent)"
            />
            Grayscale
          </label>
        </Row>

        <Row>
          <label className="flex cursor-pointer items-center gap-2 text-sm text-(--color-fg)">
            <input
              type="checkbox"
              checked={value.invert}
              onChange={(e) => onChange({ ...value, invert: e.target.checked })}
              className="accent-(--color-accent)"
            />
            Invert
          </label>
        </Row>

        <Row>
          <Label htmlFor="tr-blur">Blur</Label>
          <RangeWithValue
            id="tr-blur"
            min={0}
            max={20}
            value={blurValue}
            onChange={(v) => onChange({ ...value, blur: v === 0 ? null : v })}
          />
        </Row>
      </fieldset>

      {/* ---- Output encoding ---- */}
      <fieldset className="flex flex-col gap-3">
        <legend className="mb-1 text-xs font-semibold uppercase tracking-widest text-(--color-muted)">Output</legend>

        <Row>
          <Label htmlFor="tr-enc-format">Format</Label>
          <Select
            id="tr-enc-format"
            value={value.encode.format}
            onChange={(v) => onChange({ ...value, encode: { ...value.encode, format: v as ConvertFormat } })}
          >
            <option value="webp">WebP</option>
            <option value="webpLossless">WebP (lossless)</option>
            <option value="avif">AVIF</option>
            <option value="jpeg">JPEG</option>
            <option value="png">PNG</option>
          </Select>
        </Row>

        <Row>
          <Label htmlFor="tr-enc-quality">Quality</Label>
          <RangeWithValue
            id="tr-enc-quality"
            min={1}
            max={100}
            value={value.encode.quality}
            onChange={(q) => onChange({ ...value, encode: { ...value.encode, quality: q } })}
          />
        </Row>
      </fieldset>
    </Panel>
  )
}
