import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import type { HuggingFaceLoraFacts, HuggingFaceLookupResult } from '../../../core/core-contract';

/**
 * The read-out for a queried LoRA source (v4
 * `components/image-profiles/LoraQueryResult.tsx` at `2ece98c90`).
 *
 * Shows what HuggingFace declares about a repository and **passes no judgement
 * on whether it will work here**. Which adapters suit which provider model is
 * a question of matching a provider's model ids against HuggingFace's
 * `base_model` strings — two naming conventions that answer to nobody — so the
 * facts are laid out and the reader draws the conclusion. The card itself is
 * always one click away for anyone who wants the whole story.
 *
 * The one thing this panel offers to *do* is fill in the trigger phrase, since
 * `instance_prompt` is exactly that field and is otherwise buried in a model
 * card. It never touches the Source field.
 *
 * v4's copy lives in JSX; the sentences that carry meaning are lifted into the
 * exported helpers below so the spec can pin their bytes directly rather than
 * through a whitespace-collapsed `textContent`. The helpers are 1:1 with v4's
 * own module-level `failureCopy` / `kindCopy`.
 */

/** What went wrong, in terms the reader can act on (v4 `failureCopy`). */
export function failureCopy(result: Extract<HuggingFaceLookupResult, { ok: false }>): string {
  switch (result.reason) {
    case 'not-a-repo-id':
      return 'That source carries no HuggingFace address, so there is no registry to consult. Weights hosted elsewhere must be taken on trust.';
    case 'missing-or-private':
      return 'HuggingFace declines to confirm this one. Either no such repository exists, or it is private and you are not on the list — the registry answers both cases identically, and does so on purpose. Check the spelling first; if it is a private or gated repository, a HuggingFace token will settle the question.';
    case 'not-found':
      return 'No such repository. Your token was accepted, so this is a genuine absence rather than a door held shut.';
    case 'rate-limited':
      return 'HuggingFace begs a moment’s patience — too many enquiries too quickly. Try again shortly.';
    case 'timeout':
      return 'HuggingFace did not answer within ten seconds. The registry may be having a trying afternoon.';
    case 'network':
      return 'HuggingFace could not be reached at all. Check that this machine can see the outside world.';
    default:
      return 'HuggingFace answered, but not in any language this establishment recognises.';
  }
}

/** How the repository describes its own nature (v4 `kindCopy`). */
export function kindCopy(facts: HuggingFaceLoraFacts): string {
  if (facts.isLora) return 'Tagged a LoRA adapter.';
  if (facts.isAdapter) return 'Tagged an adapter, though not specifically a LoRA.';
  return 'Not tagged as an adapter at all — this may be a full checkpoint rather than something to layer on top of one.';
}

/** The `Trained on` value: the card's list, or v4's stands-in sentence. */
export function baseModelsCopy(facts: HuggingFaceLoraFacts): string {
  return facts.baseModels.length > 0
    ? facts.baseModels.join(', ')
    : 'The card names no base model. Whether it suits your chosen model is a matter for the model card.';
}

/**
 * The `Gated` sentence. The second half turns on whether the SELECTED MODEL
 * has anywhere to put a token — the cross-reference to the options panel is
 * the only thing that makes the row actionable.
 */
export function gatedCopy(facts: HuggingFaceLoraFacts, supportsPrivateWeightsToken: boolean): string {
  const tail = supportsPrivateWeightsToken
    ? 'The selected model accepts one — see the HuggingFace Token field in the options above.'
    : 'The selected model has nowhere to put one, so these weights are unlikely to load.';
  return `This repository is gated (${facts.gated}); the weights want a HuggingFace token. ${tail}`;
}

/** The `Standing` line — `likes · downloads`, either half omittable. */
export function standingCopy(facts: HuggingFaceLoraFacts): string {
  return [
    facts.likes !== null ? `${facts.likes.toLocaleString()} likes` : null,
    facts.downloads !== null ? `${facts.downloads.toLocaleString()} downloads` : null,
  ]
    .filter(Boolean)
    .join(' · ');
}

@Component({
  selector: 'qt-lora-query-result',
  changeDetection: ChangeDetectionStrategy.OnPush,
  styles: [':host { display: block; }'],
  template: `
    @if (failure(); as failed) {
      <div class="rounded border qt-border-warning qt-bg-surface-alt p-3 space-y-2">
        <p class="qt-text-label-xs">{{ failureHeading() }}</p>
        <p class="qt-text-xs">{{ failureSentence() }}</p>
        @if (failed.url) {
          <!-- A link out to the model card, opened away from the half-filled form. -->
          <a
            [href]="failed.url"
            target="_blank"
            rel="noopener noreferrer"
            class="qt-link text-xs"
            >Try the page yourself ↗</a
          >
        }
      </div>
    } @else if (facts(); as f) {
      <div class="rounded border qt-border-default qt-bg-surface-alt p-3 space-y-2">
        <div class="flex items-center justify-between gap-2">
          <p class="qt-text-label-xs">HuggingFace says</p>
          <a [href]="f.url" target="_blank" rel="noopener noreferrer" class="qt-link text-xs"
            >{{ f.repoId }} ↗</a
          >
        </div>

        <dl class="space-y-1 qt-text-xs">
          <div class="flex gap-2">
            <dt class="w-28 shrink-0 qt-text-secondary">Trained on</dt>
            <dd>{{ baseModels() }}</dd>
          </div>
          <div class="flex gap-2">
            <dt class="w-28 shrink-0 qt-text-secondary">Nature</dt>
            <dd>{{ kind() }}</dd>
          </div>
          @if (f.pipelineTag) {
            <div class="flex gap-2">
              <dt class="w-28 shrink-0 qt-text-secondary">Pipeline</dt>
              <dd>{{ f.pipelineTag }}</dd>
            </div>
          }
          <div class="flex gap-2">
            <dt class="w-28 shrink-0 qt-text-secondary">Weights</dt>
            <dd>
              @if (f.weightFiles.length === 0) {
                <span class="qt-text-warning"
                  >No .safetensors file in the repository — the weights may live elsewhere, or under
                  another name.</span
                >
              } @else if (f.weightFiles.length === 1) {
                {{ f.weightFiles[0] }}
              } @else {
                {{ f.weightFiles.join(', ') }}
                <span class="qt-text-warning">
                  — more than one, so a bare owner/name leaves the choice to your provider. Name the
                  file directly if you have a preference.</span
                >
              }
            </dd>
          </div>
          @if (f.gated !== false) {
            <div class="flex gap-2">
              <dt class="w-28 shrink-0 qt-text-secondary">Gated</dt>
              <dd class="qt-text-warning">{{ gated() }}</dd>
            </div>
          }
          @if (f.downloads !== null || f.likes !== null) {
            <div class="flex gap-2">
              <dt class="w-28 shrink-0 qt-text-secondary">Standing</dt>
              <dd>{{ standing() }}</dd>
            </div>
          }
        </dl>

        @if (f.triggerPhrase) {
          <div class="flex flex-wrap items-center gap-2 border-t qt-border-default pt-2">
            <span class="qt-text-xs">
              Declared trigger phrase: <code>{{ f.triggerPhrase }}</code>
            </span>
            @if (phraseOnOffer(); as phrase) {
              <button
                type="button"
                class="qt-button px-2 py-1 qt-button-secondary text-xs"
                (click)="useTriggerPhrase.emit(phrase)"
              >
                Use it
              </button>
            } @else {
              <span class="qt-text-xs qt-text-success">— already in place.</span>
            }
          </div>
        }

        <p class="qt-text-xs qt-text-secondary">
          This is what the registry declares, and nothing more. Whether these weights agree with your
          chosen model is between you and your provider — read the card if in doubt.
        </p>
      </div>
    }
  `,
})
export class LoraQueryResult {
  readonly result = input.required<HuggingFaceLookupResult>();
  /** Whether the selected model has anywhere to put a token for gated weights. */
  readonly supportsPrivateWeightsToken = input<boolean>(false);
  /** The row's current trigger phrase, so an identical one is not re-offered. */
  readonly currentTriggerPhrase = input<string>('');
  readonly useTriggerPhrase = output<string>();

  protected readonly failure = computed(() => {
    const r = this.result();
    return r.ok ? null : r;
  });

  protected readonly facts = computed(() => {
    const r = this.result();
    return r.ok ? r.facts : null;
  });

  protected readonly failureHeading = computed(() => {
    const f = this.failure();
    return f?.repoId ? `HuggingFace — ${f.repoId}` : 'HuggingFace';
  });

  /** Named apart from the module-level `failureCopy` it calls, so a refactor
   * cannot turn the call into silent self-recursion. */
  protected readonly failureSentence = computed(() => {
    const f = this.failure();
    return f ? failureCopy(f) : '';
  });

  protected readonly baseModels = computed(() => {
    const f = this.facts();
    return f ? baseModelsCopy(f) : '';
  });

  protected readonly kind = computed(() => {
    const f = this.facts();
    return f ? kindCopy(f) : '';
  });

  protected readonly gated = computed(() => {
    const f = this.facts();
    return f ? gatedCopy(f, this.supportsPrivateWeightsToken()) : '';
  });

  protected readonly standing = computed(() => {
    const f = this.facts();
    return f ? standingCopy(f) : '';
  });

  /**
   * v4 `:88-89` — the button is offered only when the card declares a phrase
   * AND it differs from what the row already holds, compared against the
   * TRIMMED current value.
   */
  protected readonly phraseOnOffer = computed(() => {
    const f = this.facts();
    if (!f?.triggerPhrase) return null;
    return f.triggerPhrase !== this.currentTriggerPhrase().trim() ? f.triggerPhrase : null;
  });
}
