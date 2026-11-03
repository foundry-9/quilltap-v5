// Transcribed case for case from v4 `__tests__/unit/components/chat/diagrams/export-png.test.ts` (236c04e35); v5-authored cases are prefixed `v5:`.
/**
 * Preparing a drawing for Copy PNG: the three things an SVG image cannot see
 * of the page — HTML labels in <foreignObject>, web fonts, and inherited
 * colour — and the canvas-size ceiling. The paint itself needs a real canvas
 * and is exercised in the browser.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  clearEmbeddedFontCache,
  parseFontFamilies,
  prepareSvgForRaster,
  safeExportScale,
} from './export-png'

const SVG_NS = 'http://www.w3.org/2000/svg'

function mount(markup: string): SVGSVGElement {
  const host = document.createElement('div')
  host.innerHTML = markup
  document.body.appendChild(host)
  return host.querySelector('svg') as SVGSVGElement
}

function parse(markup: string): Document {
  return new DOMParser().parseFromString(markup, 'image/svg+xml')
}

/**
 * v5: this jsdom's cssstyle drops an `@font-face` rule's `src` descriptor
 * (`style.getPropertyValue('src')` is `''`, measured 2026-11-03), where the
 * jsdom under v4's jest kept it and every browser does. Hand back the `src`
 * the sheet wrote, so the font cases exercise the embedding they describe.
 */
function browserLikeFontSrc(srcByFamily: Record<string, string>): void {
  const real = CSSStyleDeclaration.prototype.getPropertyValue
  vi.spyOn(CSSStyleDeclaration.prototype, 'getPropertyValue').mockImplementation(function (
    this: CSSStyleDeclaration,
    name: string,
  ) {
    const value = real.call(this, name)
    if (name !== 'src' || value) return value
    const family = real.call(this, 'font-family').replace(/^["']|["']$/g, '')
    return srcByFamily[family] ?? value
  })
}

describe('prepareSvgForRaster', () => {
  const originalFetch = globalThis.fetch

  beforeEach(() => {
    clearEmbeddedFontCache()
    document.body.innerHTML = ''
    document.head.querySelectorAll('style').forEach((s) => s.remove())
  })

  afterEach(() => {
    globalThis.fetch = originalFetch
    vi.restoreAllMocks()
  })

  it('sizes the clone at twice the natural size and keeps the viewBox', async () => {
    const live = mount(`<svg xmlns="${SVG_NS}" width="100%" viewBox="0 0 120 60" style="max-width: 120px;"><rect width="10" height="10"/></svg>`)
    const prepared = await prepareSvgForRaster(live)
    expect(prepared).toMatchObject({ width: 240, height: 120, scale: 2 })
    const root = parse(prepared.markup).documentElement
    expect(root.getAttribute('width')).toBe('240')
    expect(root.getAttribute('height')).toBe('120')
    expect(root.getAttribute('viewBox')).toBe('0 0 120 60')
    expect(root.getAttribute('style') ?? '').not.toMatch(/max-width/)
    // The page's drawing is left exactly as it was.
    expect(live.getAttribute('width')).toBe('100%')
  })

  it('redraws a <foreignObject> label as centred SVG text, one tspan per line', async () => {
    const live = mount(
      `<svg xmlns="${SVG_NS}" viewBox="0 0 200 100">` +
        `<foreignObject x="20" y="10" width="100" height="40"><div xmlns="http://www.w3.org/1999/xhtml"><span>Fill the</span><br/><span>kettle</span></div></foreignObject>` +
        `</svg>`,
    )
    // jsdom has no innerText; give the label the lines a browser would.
    const div = live.querySelector('div') as HTMLElement
    Object.defineProperty(div, 'innerText', { value: 'Fill the\nkettle', configurable: true })

    const prepared = await prepareSvgForRaster(live)
    expect(prepared.replacedForeignObjects).toBe(1)
    const doc = parse(prepared.markup)
    expect(doc.getElementsByTagNameNS(SVG_NS, 'foreignObject')).toHaveLength(0)
    const text = doc.getElementsByTagNameNS(SVG_NS, 'text')[0]
    expect(text.getAttribute('text-anchor')).toBe('middle')
    const tspans = Array.from(text.getElementsByTagNameNS(SVG_NS, 'tspan'))
    expect(tspans.map((t) => t.textContent)).toEqual(['Fill the', 'kettle'])
    // Centred on the island: x = 20 + 100/2, the two lines either side of y = 10 + 40/2.
    expect(tspans.every((t) => t.getAttribute('x') === '70')).toBe(true)
    const ys = tspans.map((t) => parseFloat(t.getAttribute('y') ?? ''))
    expect((ys[0] + ys[1]) / 2).toBeCloseTo(30)
  })

  it('drops a <foreignObject> whose <switch> already has an SVG-text fallback', async () => {
    const live = mount(
      `<svg xmlns="${SVG_NS}" viewBox="0 0 10 10"><switch>` +
        `<foreignObject width="10" height="10"><div xmlns="http://www.w3.org/1999/xhtml">HTML</div></foreignObject>` +
        `<text x="1" y="5">Fallback</text></switch></svg>`,
    )
    const doc = parse((await prepareSvgForRaster(live)).markup)
    expect(doc.getElementsByTagNameNS(SVG_NS, 'foreignObject')).toHaveLength(0)
    const texts = Array.from(doc.getElementsByTagNameNS(SVG_NS, 'text')).map((t) => t.textContent)
    expect(texts).toEqual(['Fallback'])
  })

  it('gives the root the live colour, so currentColor ink survives', async () => {
    const host = document.createElement('div')
    host.style.color = 'rgb(10, 20, 30)'
    host.innerHTML = `<svg xmlns="${SVG_NS}" viewBox="0 0 10 10"><path d="M0 0L10 10" stroke="currentColor"/></svg>`
    document.body.appendChild(host)
    const prepared = await prepareSvgForRaster(host.querySelector('svg') as SVGSVGElement)
    expect(parse(prepared.markup).documentElement.getAttribute('style')).toMatch(/color:\s*rgb\(10, 20, 30\)/)
  })

  it('embeds the page\'s web font for a family the drawing uses', async () => {
    const style = document.createElement('style')
    style.textContent =
      '@font-face { font-family: "Inter"; src: url("/fonts/Inter-Regular.woff2") format("woff2"); font-weight: 400; }\n' +
      '@font-face { font-family: "Unused"; src: url("/fonts/Unused.woff2") format("woff2"); }'
    document.head.appendChild(style)
    browserLikeFontSrc({
      Inter: 'url("/fonts/Inter-Regular.woff2") format("woff2")',
      Unused: 'url("/fonts/Unused.woff2") format("woff2")',
    })
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      blob: async () => new Blob(['font-bytes'], { type: 'font/woff2' }),
    })
    globalThis.fetch = fetchMock as unknown as typeof fetch

    // (jsdom does not inherit font-family, so the text states it.)
    const live = mount(`<svg xmlns="${SVG_NS}" viewBox="0 0 10 10"><text x="1" y="5" style="font-family: 'Inter', sans-serif">Hi</text></svg>`)

    const prepared = await prepareSvgForRaster(live)
    expect(prepared.embeddedFamilies).toEqual(['Inter'])
    expect(fetchMock).toHaveBeenCalledTimes(1)
    expect(String(fetchMock.mock.calls[0][0])).toMatch(/\/fonts\/Inter-Regular\.woff2$/)
    const embedded = parse(prepared.markup).getElementsByTagNameNS(SVG_NS, 'style')[0]?.textContent ?? ''
    expect(embedded).toMatch(/@font-face\{font-family:"Inter";src:url\("data:font\/woff2;base64,/)
    expect(embedded).toMatch(/font-weight:400;/)
    expect(embedded).not.toMatch(/Unused/)
  })

  it('still prepares the drawing when a font cannot be fetched', async () => {
    const style = document.createElement('style')
    style.textContent = '@font-face { font-family: "Inter"; src: url("/fonts/Inter-Regular.woff2") format("woff2"); }'
    document.head.appendChild(style)
    browserLikeFontSrc({ Inter: 'url("/fonts/Inter-Regular.woff2") format("woff2")' })
    const debug = vi.spyOn(console, 'debug').mockImplementation(() => undefined)
    globalThis.fetch = vi.fn().mockRejectedValue(new Error('offline')) as unknown as typeof fetch

    const live = mount(`<svg xmlns="${SVG_NS}" viewBox="0 0 10 10" style="font-family: Inter"><text>Hi</text></svg>`)

    const prepared = await prepareSvgForRaster(live)
    expect(prepared.embeddedFamilies).toEqual([])
    expect(parse(prepared.markup).getElementsByTagNameNS(SVG_NS, 'style')).toHaveLength(0)
    // v5: the face WAS found — the fetch is what failed, and it says so.
    expect(debug).toHaveBeenCalledWith('[diagrams] Could not embed a font for PNG export', {
      url: expect.stringMatching(/\/fonts\/Inter-Regular\.woff2$/),
      error: 'offline',
    })
  })
})

describe('parseFontFamilies', () => {
  it('splits a stack, unquotes it, and leaves generic families to the system', () => {
    expect(parseFontFamilies('"EB Garamond", \'Segoe Print\', Inter, serif, system-ui')).toEqual([
      'eb garamond',
      'segoe print',
      'inter',
    ])
  })
})

describe('safeExportScale', () => {
  it('keeps 2× for an ordinary drawing', () => {
    expect(safeExportScale({ width: 800, height: 600 }, 2)).toBe(2)
  })

  it('backs off for a drawing too large for a canvas at 2×', () => {
    const scale = safeExportScale({ width: 5000, height: 4000 }, 2)
    expect(scale).toBeLessThan(2)
    expect(5000 * scale * 4000 * scale).toBeLessThanOrEqual(16_777_216 + 1)
  })
})
