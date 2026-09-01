import { ChangeDetectionStrategy, Component, computed, inject, input, output, signal } from '@angular/core';

import { CoreClient } from '../../../core/core-client';
import type {
  HuggingFaceLookupResult,
  ImageLoraSpec,
  ImageLoraSupport,
} from '../../../core/core-contract';
import { extractHuggingFaceRepoId } from './huggingface-repo-id';
import { queryLoraMetadata } from './image-profiles.api';
import { LoraQueryResult } from './lora-query-result';

/**
 * The LoRA list editor for an image profile (v4
 * `components/image-profiles/LoraListEditor.tsx` at `84f33ce94` + `2ece98c90`).
 *
 * Shown only when the selected provider/model resolves an `ImageLoraSupport`
 * — the host resolves that server-side and hands it down, so the browser never
 * re-implements the exact-id / family-prefix / provider-constraint lookup.
 *
 * Rows write straight into `parameters.loras` in the canonical
 * `{ source, scale, triggerPhrase }` shape. Over-cap rows (a profile saved
 * against a four-adapter model, then pointed at a one-adapter model) are
 * flagged, not deleted: switching the model back must lose nothing, and the
 * request builder caps the list again at generation time anyway.
 *
 * Each row can **Query** its source against HuggingFace. That is a read-out,
 * not a gate: it never blocks saving, never rewrites the Source field, and
 * offers no opinion on whether the adapter suits the selected model — see
 * {@link LoraQueryResult} for why such an opinion would be a liability. A
 * stale answer is worse than none, so a row's result is cleared the moment its
 * source is edited.
 */

/** Mirrors `DEFAULT_LORA_SCALE` on the server side. */
export const DEFAULT_SCALE = { min: 0, max: 2, default: 1, step: 0.05 };

export interface LoraScaleBounds {
  min: number;
  max: number;
  default: number;
  step: number;
}

/** v4 `scaleBounds` — a declared `step` is optional; the rest are not. */
export function scaleBounds(support: ImageLoraSupport): LoraScaleBounds {
  const declared = support.scale;
  if (!declared) return { ...DEFAULT_SCALE };
  return {
    min: declared.min,
    max: declared.max,
    default: declared.default,
    step: declared.step ?? DEFAULT_SCALE.step,
  };
}

/** v4 `sourceHint` — what this provider will accept in a Source box. */
export function sourceHint(support: ImageLoraSupport): string {
  const kinds = support.sourceKinds;
  const parts: string[] = [];
  if (kinds.includes('url')) parts.push('a .safetensors URL');
  if (kinds.includes('hf-repo')) parts.push('a HuggingFace owner/model-name');
  if (kinds.includes('provider-id')) parts.push("an identifier from the provider's own catalogue");
  if (parts.length === 0) return 'Whatever identifier this provider accepts.';
  if (parts.length === 1) return `${parts[0][0].toUpperCase()}${parts[0].slice(1)}.`;
  return `${parts.slice(0, -1).join(', ')} or ${parts[parts.length - 1]}.`;
}

/** Per-row lookup state, keyed by row index (v4 `RowQuery`). */
interface RowQuery {
  loading: boolean;
  result: HuggingFaceLookupResult | null;
}

/** One row with everything the template needs already resolved. */
interface PreparedRow {
  index: number;
  lora: ImageLoraSpec;
  overCap: boolean;
  scale: number;
  /** Non-null exactly when there is a repository to ask about. */
  repoId: string | null;
  query: RowQuery | undefined;
  triggerPhrase: string;
}

@Component({
  selector: 'qt-lora-list-editor',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [LoraQueryResult],
  styles: [':host { display: block; }'],
  template: `
    @if (support(); as s) {
      <div class="space-y-4 border-t qt-border-default pt-4">
        <div>
          <h3 class="text-sm qt-text-primary">LoRA Adapters (Optional)</h3>
          <p class="qt-text-xs mt-1">{{ intro() }}</p>
        </div>

        @if (loras().length === 0) {
          <p class="qt-text-xs">
            No adapters attached — the model generates in its own native manner.
          </p>
        }

        @for (row of rows(); track row.index) {
          <div
            class="space-y-2 rounded border p-3"
            [class.qt-border-warning]="row.overCap"
            [class.qt-border-default]="!row.overCap"
          >
            <div class="flex items-center justify-between gap-2">
              <span class="qt-text-label-xs">{{ adapterLabel(row) }}</span>
              <button
                type="button"
                class="qt-button px-2 py-1 qt-button-secondary text-xs"
                (click)="remove(row.index)"
              >
                Remove
              </button>
            </div>

            @if (row.overCap) {
              <p class="qt-text-warning text-xs">{{ overCapWarning() }}</p>
            }

            <div>
              <label class="qt-text-label-xs" [attr.for]="'lora-source-' + row.index">Source</label>
              <div class="flex items-start gap-2">
                <input
                  [id]="'lora-source-' + row.index"
                  type="text"
                  class="qt-input text-sm"
                  placeholder="owner/model-name or https://…/weights.safetensors"
                  [value]="row.lora.source"
                  (input)="setSource(row.index, $any($event.target).value)"
                />
                <button
                  type="button"
                  class="qt-button shrink-0 px-3 py-2 qt-button-secondary text-xs"
                  [disabled]="!row.repoId || row.query?.loading"
                  [title]="queryTitle(row)"
                  (click)="runQuery(row.index, row.lora.source)"
                >
                  {{ row.query?.loading ? 'Asking…' : 'Query' }}
                </button>
              </div>
              <p class="qt-text-xs">
                Querying asks HuggingFace what it declares about this adapter — its base model, its
                weights, its magic word. It settles nothing about whether the two of you will get
                along.
              </p>
            </div>

            <div>
              <label class="qt-text-label-xs" [attr.for]="'lora-scale-' + row.index"
                >Strength — {{ row.scale.toFixed(2) }}</label
              >
              <!-- §B: the qt-range class is P4.D142's; this editor ships no slider CSS. -->
              <input
                [id]="'lora-scale-' + row.index"
                type="range"
                class="qt-range w-full"
                [min]="bounds().min"
                [max]="bounds().max"
                [step]="bounds().step"
                [value]="row.scale"
                (input)="setScale(row.index, $any($event.target).value)"
              />
              <p class="qt-text-xs">{{ scaleHint() }}</p>
            </div>

            <div>
              <label class="qt-text-label-xs" [attr.for]="'lora-trigger-' + row.index"
                >Trigger Phrase (optional)</label
              >
              <input
                [id]="'lora-trigger-' + row.index"
                type="text"
                class="qt-input text-sm"
                placeholder="e.g., in the style of ohwx"
                [value]="row.triggerPhrase"
                (input)="setTriggerPhrase(row.index, $any($event.target).value)"
              />
              <p class="qt-text-xs">
                Many adapters answer only to a magic word. Whatever you put here is woven into the
                prompt on every generation that uses this profile.
              </p>
            </div>

            @if (row.query?.result; as result) {
              <qt-lora-query-result
                [result]="result"
                [supportsPrivateWeightsToken]="s.supportsPrivateWeightsToken === true"
                [currentTriggerPhrase]="row.triggerPhrase"
                (useTriggerPhrase)="setTriggerPhrase(row.index, $event)"
              />
            }
          </div>
        }

        <div class="flex items-center gap-3">
          <button
            type="button"
            class="qt-button px-3 py-2 qt-button-primary"
            [disabled]="atCap()"
            [title]="addTitle()"
            (click)="add()"
          >
            Add LoRA
          </button>
          <span class="qt-text-xs">{{ loras().length }} of {{ s.maxLoras }}</span>
        </div>
      </div>
    }
  `,
})
export class LoraListEditor {
  private readonly core = inject(CoreClient);

  /** Resolved support for the selected model; null hides the editor entirely. */
  readonly support = input<ImageLoraSupport | null>(null);
  /** Current value of `parameters.loras`. */
  readonly loras = input<ImageLoraSpec[]>([]);
  /**
   * The profile's configured `hf_api_token`, when it has one. Passed through
   * to the lookup so gated and private repositories resolve for the people
   * entitled to see them; it rides the request body, never the query string.
   */
  readonly hfToken = input<string | undefined>(undefined);

  readonly lorasChange = output<ImageLoraSpec[]>();

  private readonly queries = signal<Record<number, RowQuery>>({});

  protected readonly bounds = computed<LoraScaleBounds>(() => {
    const s = this.support();
    return s ? scaleBounds(s) : { ...DEFAULT_SCALE };
  });

  protected readonly atCap = computed(() => {
    const s = this.support();
    return s ? this.loras().length >= s.maxLoras : false;
  });

  protected readonly intro = computed(() => {
    const s = this.support();
    if (!s) return '';
    const capacity = s.maxLoras === 1 ? 'a single adapter' : `up to ${s.maxLoras} adapters`;
    return `Adapters are applied in the order listed. ${sourceHint(s)} This model accepts ${capacity}.`;
  });

  protected readonly overCapWarning = computed(() => {
    const s = this.support();
    return `Beyond this model's limit of ${s?.maxLoras ?? 0} — kept on the profile, but left behind on every request until you remove an earlier adapter or return to a model that takes more.`;
  });

  protected readonly scaleHint = computed(() => {
    const b = this.bounds();
    return `${b.min} to ${b.max}; this model's own default is ${b.default}.`;
  });

  protected readonly addTitle = computed(() =>
    this.atCap() ? `This model accepts at most ${this.support()?.maxLoras}` : 'Attach another adapter',
  );

  protected readonly rows = computed<PreparedRow[]>(() => {
    const s = this.support();
    if (!s) return [];
    const bounds = this.bounds();
    const queries = this.queries();
    return this.loras().map((lora, index) => ({
      index,
      lora,
      overCap: index >= s.maxLoras,
      scale: typeof lora.scale === 'number' ? lora.scale : bounds.default,
      // The button is offered only when there is a repository to ask about.
      // A weights URL on some other host has no card behind it.
      repoId: extractHuggingFaceRepoId(lora.source),
      query: queries[index],
      triggerPhrase: lora.triggerPhrase ?? '',
    }));
  });

  protected adapterLabel(row: PreparedRow): string {
    return `Adapter ${row.index + 1}${row.lora.label ? ` — ${row.lora.label}` : ''}`;
  }

  protected queryTitle(row: PreparedRow): string {
    return row.repoId
      ? `Ask HuggingFace about ${row.repoId}`
      : 'Only a HuggingFace owner/model-name (or a huggingface.co address) can be looked up';
  }

  /**
   * v4 `update` (`:87-99`). An answer about the previous source would be
   * actively misleading beside a new one, so editing the source discards it —
   * and ONLY editing the source: a scale or trigger-phrase edit leaves the
   * row's findings standing.
   */
  private update(index: number, patch: Partial<ImageLoraSpec>, clearsQuery: boolean): void {
    if (clearsQuery) {
      this.queries.update((prev) => {
        if (!prev[index]) return prev;
        const next = { ...prev };
        delete next[index];
        return next;
      });
    }
    this.lorasChange.emit(this.loras().map((lora, i) => (i === index ? { ...lora, ...patch } : lora)));
  }

  protected setSource(index: number, value: string): void {
    this.update(index, { source: value }, true);
  }

  protected setScale(index: number, value: string): void {
    this.update(index, { scale: Number(value) }, false);
  }

  /** v4 `:242` — an empty box means no phrase, not an empty one. */
  protected setTriggerPhrase(index: number, value: string): void {
    this.update(index, { triggerPhrase: value || undefined }, false);
  }

  /**
   * v4 `remove` (`:101-115`). Rows are keyed by position, so removing one has
   * to shuffle the results down with them — otherwise row 1's findings
   * resurface under row 0.
   */
  protected remove(index: number): void {
    this.queries.update((prev) => {
      const next: Record<number, RowQuery> = {};
      for (const [key, value] of Object.entries(prev)) {
        const at = Number(key);
        if (at === index) continue;
        next[at > index ? at - 1 : at] = value;
      }
      return next;
    });
    this.lorasChange.emit(this.loras().filter((_, i) => i !== index));
  }

  /** v4 `add` (`:117-119`) — a blank source at the model's own default scale. */
  protected add(): void {
    this.lorasChange.emit([...this.loras(), { source: '', scale: this.bounds().default }]);
  }

  /** v4 `query` (`:121-145`). */
  protected async runQuery(index: number, source: string): Promise<void> {
    this.queries.update((prev) => ({ ...prev, [index]: { loading: true, result: null } }));
    try {
      const data = await queryLoraMetadata(this.core, source, this.hfToken());
      this.queries.update((prev) => ({ ...prev, [index]: { loading: false, result: data } }));
    } catch {
      // A failed request is reported in the same panel as a failed lookup —
      // from the reader's chair they are the same disappointment.
      this.queries.update((prev) => ({
        ...prev,
        [index]: {
          loading: false,
          result: { ok: false, reason: 'network', repoId: null, url: null },
        },
      }));
    }
  }
}
