import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  ElementRef,
  InjectionToken,
  computed,
  effect,
  inject,
  input,
  signal,
  untracked,
  viewChild,
} from '@angular/core';
import { DomSanitizer } from '@angular/platform-browser';

import { ThemeService } from '../../theme/theme.service';
import { Icon } from '../../ui/icon';
import type { DiagramEngine, DiagramStyle } from './diagram-fences';
import { DiagramLightboxService } from './diagram-lightbox.service';
import { cacheKeyFor, cachePeek, cachePut } from './diagram-render-cache';
import type { RenderedDiagram } from './diagram-types';
import { renderGraphviz } from './render-graphviz';
import { renderMermaid } from './render-mermaid';
import { renderSvg } from './render-svg';

/** The outcome of the last render this block finished, and what it was for. */
type RenderResult = { key: string; svg: string } | { key: string; error: string };

export type DiagramRenderers = Record<
  DiagramEngine,
  (source: string, options: { style: DiagramStyle; host: Element }) => Promise<RenderedDiagram>
>;

const RENDERERS: DiagramRenderers = {
  mermaid: (source, options) => renderMermaid(source, options),
  graphviz: (source, options) => renderGraphviz(source, options),
  svg: (source) => renderSvg(source),
};

/**
 * The engine table. A token only so a spec can stand mocks in for the three
 * renderers, as v4's jest suite does (`DiagramBlock.test.tsx:17-19`) — the
 * Angular builder pre-bundles specs, so module mocking is not available.
 */
export const DIAGRAM_RENDERERS = new InjectionToken<DiagramRenderers>('DIAGRAM_RENDERERS', {
  providedIn: 'root',
  factory: () => RENDERERS,
});

const BLOT_NOTE = 'The draughtsman’s pen has blotted — the diagram below could not be drawn.';

/**
 * DiagramBlock — a closed diagram fence, drawn.
 *
 * The dispatcher for every diagram engine: picks the renderer for the fence's
 * language, shows a fixed-height placeholder while it works, swaps in the SVG,
 * and adds the toolbar (view source, copy source, copy SVG, download SVG).
 * Clicking the drawing (or Enlarge) opens it in the lightbox, to zoom into and
 * copy as a PNG; closing that puts the reader back where they were.
 * A fence that cannot be drawn degrades to the ordinary code block under a
 * one-line note — never a blank space, never a stack trace.
 *
 * The SVG goes into the page as markup inside a container, never as an
 * `<img>`: "Salon Images" quick-hide swaps images for placeholders, and it
 * deliberately does not reach diagrams.
 *
 * v5 port of v4 `components/chat/diagrams/DiagramBlock.tsx` (236c04e35).
 * v4's `CodeBlockWithCopy` has no v5 twin (P4.D353 R-9): the streaming,
 * failure and View-source arms show the renderer's own `<pre><code>` block —
 * `fallbackHtml`, captured by the slot pass — so the degraded state is
 * byte-identical to the un-diagrammed render. The theme key is
 * `activeThemeId|resolvedColorMode` (R-4): v5's bundled packs are fixed per id,
 * so v4's third component (a tokens-object identity) has nothing to add.
 */
@Component({
  selector: 'qt-diagram-block',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  host: { class: 'qt-diagram-block' },
  template: `
    @if (streaming()) {
      <div class="qt-diagram-fallback" [innerHTML]="fallback()"></div>
    } @else if (errorMessage() !== null) {
      <div class="qt-diagram-error" [attr.data-diagram-engine]="engine()">
        <p class="qt-diagram-error-note" [title]="errorMessage()">{{ BLOT_NOTE }}</p>
        <details class="qt-diagram-error-details">
          <summary>What the {{ engineName() }} engine said</summary>
          <pre>{{ errorMessage() }}</pre>
        </details>
        <div class="qt-diagram-fallback" [innerHTML]="fallback()"></div>
      </div>
    } @else {
      <div
        class="qt-diagram"
        [attr.data-diagram-engine]="engine()"
        [attr.data-diagram-style]="engine() === 'svg' ? null : style()"
      >
        @if (svg(); as drawn) {
          <div class="qt-diagram-toolbar" role="toolbar" aria-label="Diagram actions">
            <button
              type="button"
              class="qt-diagram-toolbar-button"
              (click)="showSource.set(!showSource())"
              [attr.aria-pressed]="showSource()"
              [title]="showSource() ? 'Hide source' : 'View source'"
            >
              <qt-icon name="code" class="w-3.5 h-3.5" />
              <span>{{ showSource() ? 'Hide source' : 'View source' }}</span>
            </button>
            <button
              type="button"
              class="qt-diagram-toolbar-button"
              (click)="handleOpen()"
              title="Enlarge — zoom, and copy as PNG"
            >
              <qt-icon name="expand" class="w-3.5 h-3.5" />
              <span>Enlarge</span>
            </button>
            <button
              type="button"
              class="qt-diagram-toolbar-button"
              (click)="handleCopySource()"
              title="Copy source"
            >
              <qt-icon [name]="copied() === 'source' ? 'check' : 'copy'" class="w-3.5 h-3.5" />
              <span>{{ copied() === 'source' ? 'Copied' : 'Copy source' }}</span>
            </button>
            <button
              type="button"
              class="qt-diagram-toolbar-button"
              (click)="handleCopySvg()"
              title="Copy SVG"
            >
              <qt-icon [name]="copied() === 'svg' ? 'check' : 'copy'" class="w-3.5 h-3.5" />
              <span>{{ copied() === 'svg' ? 'Copied' : 'Copy SVG' }}</span>
            </button>
            <button
              type="button"
              class="qt-diagram-toolbar-button"
              (click)="handleDownloadSvg()"
              title="Download SVG"
            >
              <qt-icon name="download" class="w-3.5 h-3.5" />
              <span>Download SVG</span>
            </button>
          </div>
          <div
            #canvas
            class="qt-diagram-canvas"
            role="button"
            tabindex="0"
            [attr.aria-label]="kind() + ' — open to zoom and copy'"
            title="Click to enlarge"
            (click)="handleCanvasClick()"
            (keydown)="handleCanvasKeyDown($event)"
            [innerHTML]="trusted(drawn)"
          ></div>
        } @else {
          <div class="qt-diagram-placeholder" role="status" aria-live="polite">
            Sketching the diagram…
          </div>
        }
        @if (showSource()) {
          <div class="qt-diagram-fallback" [innerHTML]="fallback()"></div>
        }
        <!-- Scratch space a renderer may lay a drawing out in (offscreen).
             Angular never puts children here, so a renderer's temporary nodes
             are safe. -->
        <div #stage class="qt-diagram-stage" aria-hidden="true"></div>
      </div>
    }
  `,
})
export class DiagramBlock {
  readonly engine = input.required<DiagramEngine>();
  /** The fence's language as written (`mermaid`, `dot`, `graphviz`, `svg`). */
  readonly language = input.required<string>();
  /** The diagram source — the fence's contents. */
  readonly source = input.required<string>();
  /** The authoring character's style (Mermaid and Graphviz only). */
  readonly style = input.required<DiagramStyle>();
  /**
   * The message is still streaming. The fence may be half-received, so show
   * it as code until the stream ends; a parse attempt here would flash the
   * error state at every token.
   */
  readonly streaming = input(false);
  /** The renderer's own `<pre><code>` block for this fence (R-9). */
  readonly fallbackHtml = input('');

  protected readonly BLOT_NOTE = BLOT_NOTE;

  private readonly sanitizer = inject(DomSanitizer);
  private readonly theme = inject(ThemeService);
  private readonly lightbox = inject(DiagramLightboxService);
  private readonly renderers = inject(DIAGRAM_RENDERERS);
  private readonly stage = viewChild<ElementRef<HTMLDivElement>>('stage');
  private readonly canvas = viewChild<ElementRef<HTMLDivElement>>('canvas');

  private readonly result = signal<RenderResult | null>(null);
  protected readonly showSource = signal(false);
  protected readonly copied = signal<'source' | 'svg' | null>(null);

  private readonly themeKey = computed(
    () => `${this.theme.activeThemeId() ?? 'default'}|${this.theme.resolvedColorMode()}`,
  );
  private readonly cacheKey = computed(() =>
    cacheKeyFor(this.engine(), this.style(), this.themeKey(), this.source()),
  );

  // A drawing already in the cache shows at once (a remounted row). Otherwise
  // the last finished render is shown — for an older key that means the
  // previous drawing stays up while a re-render (a theme change) is under way,
  // so the row doesn't collapse to the placeholder and back.
  private readonly cached = computed(() => {
    this.result(); // re-read the (non-reactive) cache whenever a render lands
    return cachePeek(this.cacheKey());
  });
  protected readonly errorMessage = computed(() => {
    const result = this.result();
    const current = result && result.key === this.cacheKey() ? result : null;
    return !this.cached() && current && 'error' in current ? current.error : null;
  });
  protected readonly svg = computed(() => {
    const result = this.result();
    return this.cached() ?? (result && 'svg' in result ? result.svg : null);
  });

  protected readonly fallback = computed(() =>
    this.sanitizer.bypassSecurityTrustHtml(this.fallbackHtml()),
  );
  protected readonly kind = computed(() => (this.engine() === 'svg' ? 'Drawing' : 'Diagram'));
  protected readonly engineName = computed(() => {
    const engine = this.engine();
    return engine === 'graphviz' ? 'Graphviz' : engine === 'mermaid' ? 'Mermaid' : 'SVG';
  });

  private readonly timers = new Set<ReturnType<typeof setTimeout>>();

  constructor() {
    effect((onCleanup) => {
      const cacheKey = this.cacheKey();
      const streaming = this.streaming();
      untracked(() => {
        if (streaming || cachePeek(cacheKey)) return;
        const engine = this.engine();
        const source = this.source();
        const style = this.style();

        let cancelled = false;
        // One frame's grace: on a theme switch the new tokens land in the
        // page's CSS a moment after the signal changes, and the palette is
        // read live.
        const frame = requestAnimationFrame(() => {
          const host = this.stage()?.nativeElement;
          if (!host) return;
          this.renderers[engine](source, { style, host })
            .then(({ svg: drawn }) => {
              cachePut(cacheKey, drawn);
              if (!cancelled) this.result.set({ key: cacheKey, svg: drawn });
            })
            .catch((err: unknown) => {
              const message = err instanceof Error ? err.message : String(err);
              console.debug('[diagrams] Diagram could not be drawn', { engine, error: message });
              if (!cancelled) this.result.set({ key: cacheKey, error: message });
            });
        });

        onCleanup(() => {
          cancelled = true;
          cancelAnimationFrame(frame);
        });
      });
    });

    inject(DestroyRef).onDestroy(() => {
      for (const timer of this.timers) clearTimeout(timer);
    });
  }

  protected trusted(svg: string) {
    return this.sanitizer.bypassSecurityTrustHtml(svg);
  }

  private flashCopied(what: 'source' | 'svg'): void {
    this.copied.set(what);
    const timer = setTimeout(() => {
      this.timers.delete(timer);
      if (this.copied() === what) this.copied.set(null);
    }, 2000);
    this.timers.add(timer);
  }

  protected async handleCopySource(): Promise<void> {
    try {
      await navigator.clipboard.writeText(this.source());
      this.flashCopied('source');
    } catch (err) {
      console.error('Failed to copy diagram source', {
        error: err instanceof Error ? err.message : String(err),
      });
    }
  }

  protected async handleCopySvg(): Promise<void> {
    const svg = this.svg();
    if (!svg) return;
    try {
      await navigator.clipboard.writeText(svg);
      this.flashCopied('svg');
    } catch (err) {
      console.error('Failed to copy diagram SVG', {
        error: err instanceof Error ? err.message : String(err),
      });
    }
  }

  protected handleDownloadSvg(): void {
    const svg = this.svg();
    if (!svg) return;
    const url = URL.createObjectURL(new Blob([svg], { type: 'image/svg+xml' }));
    const link = document.createElement('a');
    link.href = url;
    link.download = `${this.engine()}-diagram.svg`;
    document.body.appendChild(link);
    link.click();
    link.remove();
    setTimeout(() => URL.revokeObjectURL(url), 0);
  }

  protected handleOpen(): void {
    const svg = this.svg();
    const trigger = this.canvas()?.nativeElement;
    if (!svg || !trigger) return;
    this.lightbox.openLightbox({ svg, label: `${this.kind()}, enlarged`, trigger });
  }

  protected handleCanvasClick(): void {
    // A drag across a label to copy its words is not a request to enlarge.
    const selection = window.getSelection();
    if (selection && !selection.isCollapsed && selection.toString().trim() !== '') return;
    this.handleOpen();
  }

  protected handleCanvasKeyDown(e: KeyboardEvent): void {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      this.handleOpen();
    }
  }
}
