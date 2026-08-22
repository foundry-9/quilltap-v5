import { ChangeDetectionStrategy, Component, computed, input, output, signal } from '@angular/core';

import type { CharacterScenario, Pronouns } from '../../../core/core-contract';
import { MarkdownField } from '../../../editor/markdown-field';
import { PROMPT_FIELD_HINTS } from '../../../ui/prompt-field-hints';
import { PromptFieldLabel } from '../../../ui/prompt-field-label';
import type { CharacterFormData } from './character-form';
import { getPronounPreset, PRONOUN_PRESETS } from './character-form';
import { ScenarioEditor } from './scenario-editor';
import { TagChipEditor } from './tag-chip-editor';

/**
 * The Details tab (v4 `app/aurora/[id]/edit/components/CharacterBasicInfo.tsx`):
 * the system-transparency / Carina / Core-whisper toggles, name, aliases,
 * pronouns, the four vantage points (identity / description / manifesto /
 * personality — DISTINCT, never collapsed), the inline scenarios editor,
 * first message, example dialogues, the legacy system prompt, and the tag
 * editor. The seven markdown fields (identity / description / manifesto /
 * personality / first message / example dialogues / system prompt) use the
 * shared `qt-markdown-field` (v4's `MarkdownLexicalEditor`); each keeps the
 * same emit contract — a plain markdown string riding `fieldChange`, emitted
 * only on genuine edits.
 *
 * Every prompt field's label + helper is the shared `qt-prompt-field-label`
 * over `PROMPT_FIELD_HINTS` (v4 `a6870c5a`) — the hand-rolled copies are gone,
 * which is the whole point of v4's commit: the create and edit forms had
 * already drifted apart in their duplicated hint text.
 */
@Component({
  selector: 'qt-character-details-tab',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ScenarioEditor, TagChipEditor, MarkdownField, PromptFieldLabel],
  template: `
    <div class="space-y-6">
      <!-- System Transparency Switch -->
      <div class="qt-card">
        <div class="flex items-start justify-between gap-4">
          <div class="flex-1">
            <label for="systemTransparency" class="block qt-label">System transparency</label>
            <p class="text-xs qt-text-secondary mt-1">
              @if (form().systemTransparency) {
                <em>On:</em> &ldquo;My character will be able to verify everything about their
                existence, including how they are crafted and how they interact with me.&rdquo;
              } @else {
                <em>Off:</em> &ldquo;My character will trust me without being able to verify me. I
                accept the covenant of that trust.&rdquo;
              }
            </p>
            <p class="text-xs qt-text-secondary mt-2">
              When off, this character cannot use the <code>self_inventory</code> tool, cannot
              perceive announcements from the Staff (the Lantern, Aurora, the Librarian, and so on),
              and cannot reach any character vault &mdash; their own included &mdash; through the
              document tools. This setting overrides any chat- or project-level toggles for those
              three things; turning it on simply lets the chat- and project-level settings have
              their say.
            </p>
          </div>
          <label class="inline-flex items-center cursor-pointer select-none">
            <input
              id="systemTransparency"
              type="checkbox"
              [checked]="form().systemTransparency"
              (change)="
                fieldChange.emit({ ...form(), systemTransparency: $any($event.target).checked })
              "
              class="h-5 w-5 qt-accent-primary"
            />
          </label>
        </div>
      </div>

      <!-- Carina (@-query) Switch -->
      <div class="qt-card">
        <div class="flex items-start justify-between gap-4">
          <div class="flex-1">
            <label for="canBeCarina" class="block qt-label">Can answer @-queries (Carina)</label>
            <p class="text-xs qt-text-secondary mt-1">
              When enabled, this character may be invoked with <code>&#64;Name</code> in any chat to
              field a quick question by way of their personality and available tools &mdash; without
              so much as setting foot in the conversation proper.
            </p>
          </div>
          <label class="inline-flex items-center cursor-pointer select-none">
            <input
              id="canBeCarina"
              type="checkbox"
              [checked]="form().canBeCarina"
              (change)="fieldChange.emit({ ...form(), canBeCarina: $any($event.target).checked })"
              class="h-5 w-5 qt-accent-primary"
            />
          </label>
        </div>
      </div>

      <!-- Aurora's Core Whisper — per-character override.
           STACKED, not side-by-side (v4 4.8.2 fe63547a): qt-select is
           full-width, so in the two-column layout it claimed nearly the whole
           card and wrapped the label into a column a few characters wide.
           Label and description span the card; the dropdown sits beneath them.
           The two checkbox cards above keep the side-by-side layout — a
           checkbox is narrow enough for it. -->
      <div class="qt-card">
        <label for="coreWhisperEnabled" class="block qt-label">Aurora&apos;s Core whisper</label>
        <p class="text-xs qt-text-secondary mt-1 mb-3">
          Whether Aurora periodically re-offers this character their own
          <code>Core/</code> vault folder before they next take the floor.
          <em>Inherit</em> defers to the per-chat and global settings; explicit values override
          both.
        </p>
        <select
          id="coreWhisperEnabled"
          class="qt-select"
          [value]="coreWhisperSelectValue()"
          (change)="onCoreWhisperChange($any($event.target).value)"
        >
          <option value="inherit">Inherit (default)</option>
          <option value="on">Always offered</option>
          <option value="off">Never offered</option>
        </select>
      </div>

      <!-- Name -->
      <div>
        <label for="name" class="block qt-label mb-2">Name *</label>
        <input
          type="text"
          id="name"
          class="qt-input"
          required
          [value]="form().name"
          (input)="fieldChange.emit({ ...form(), name: $any($event.target).value })"
        />
      </div>

      <!-- Aliases -->
      <div>
        <label class="block qt-label mb-2">Aliases (Optional)</label>
        <p class="text-xs qt-text-secondary mb-2">
          Alternate names this character goes by. Press Enter to add.
        </p>
        <div class="flex flex-wrap gap-2 mb-2">
          @for (alias of form().aliases; track alias; let i = $index) {
            <span
              class="inline-flex items-center gap-1 rounded-full qt-bg-muted px-3 py-1 text-sm text-foreground"
            >
              {{ alias }}
              <button
                type="button"
                class="ml-1 qt-text-secondary hover:text-foreground"
                (click)="removeAlias(i)"
              >
                &times;
              </button>
            </span>
          }
        </div>
        <input
          type="text"
          class="qt-input"
          placeholder="Type an alias and press Enter"
          [value]="aliasDraft()"
          (input)="aliasDraft.set($any($event.target).value)"
          (keydown.enter)="addAlias($event)"
        />
      </div>

      <!-- Pronouns -->
      <div>
        <label class="block qt-label mb-2">Pronouns (Optional)</label>
        <p class="text-xs qt-text-secondary mb-2">
          The character&apos;s pronouns, included in system prompts so the LLM uses them correctly.
        </p>
        <select
          class="qt-select"
          [value]="pronounPreset()"
          (change)="onPronounPresetChange($any($event.target).value)"
        >
          @for (preset of pronounPresets; track preset.label) {
            <option [value]="preset.label">{{ preset.label }}</option>
          }
        </select>
        @if (form().pronouns && pronounPreset() === 'Custom') {
          <div class="mt-2 grid grid-cols-3 gap-2">
            <div>
              <label class="block text-xs qt-text-secondary mb-1">Subject</label>
              <input
                type="text"
                class="qt-input"
                placeholder="e.g., they"
                [value]="form().pronouns!.subject"
                (input)="setCustomPronoun('subject', $any($event.target).value)"
              />
            </div>
            <div>
              <label class="block text-xs qt-text-secondary mb-1">Object</label>
              <input
                type="text"
                class="qt-input"
                placeholder="e.g., them"
                [value]="form().pronouns!.object"
                (input)="setCustomPronoun('object', $any($event.target).value)"
              />
            </div>
            <div>
              <label class="block text-xs qt-text-secondary mb-1">Possessive</label>
              <input
                type="text"
                class="qt-input"
                placeholder="e.g., their"
                [value]="form().pronouns!.possessive"
                (input)="setCustomPronoun('possessive', $any($event.target).value)"
              />
            </div>
          </div>
        }
      </div>

      <!-- Title -->
      <div>
        <label for="title" class="block qt-label mb-2">Title (Optional)</label>
        <input
          type="text"
          id="title"
          class="qt-input"
          placeholder="Your private label for this character — e.g., the protagonist, the rival, the love interest. Not how strangers refer to them."
          [value]="form().title"
          (input)="fieldChange.emit({ ...form(), title: $any($event.target).value })"
        />
      </div>

      <!-- Identity -->
      <div>
        <qt-prompt-field-label [hint]="hints.identity" optional />
        <qt-markdown-field
          ariaLabel="Identity"
          minHeight="6rem"
          [value]="form().identity"
          (contentChange)="fieldChange.emit({ ...form(), identity: $event })"
        />
      </div>

      <!-- Description -->
      <div>
        <qt-prompt-field-label [hint]="hints.description" optional />
        <qt-markdown-field
          ariaLabel="Description"
          minHeight="8rem"
          [value]="form().description"
          (contentChange)="fieldChange.emit({ ...form(), description: $event })"
        />
      </div>

      <!-- Manifesto -->
      <div>
        <qt-prompt-field-label [hint]="hints.manifesto" optional />
        <qt-markdown-field
          ariaLabel="Manifesto"
          minHeight="8rem"
          [value]="form().manifesto"
          (contentChange)="fieldChange.emit({ ...form(), manifesto: $event })"
        />
      </div>

      <!-- Personality -->
      <div>
        <qt-prompt-field-label [hint]="hints.personality" optional />
        <qt-markdown-field
          ariaLabel="Personality"
          minHeight="8rem"
          [value]="form().personality"
          (contentChange)="fieldChange.emit({ ...form(), personality: $event })"
        />
      </div>

      <!-- Scenarios -->
      <qt-scenario-editor
        [scenarios]="form().scenarios"
        (scenariosChange)="onScenariosChange($event)"
      />

      <!-- First Message -->
      <div>
        <qt-prompt-field-label [hint]="hints.firstMessage" optional />
        <qt-markdown-field
          ariaLabel="First Message"
          minHeight="6rem"
          [value]="form().firstMessage"
          (contentChange)="fieldChange.emit({ ...form(), firstMessage: $event })"
        />
      </div>

      <!-- Example Dialogues -->
      <div>
        <qt-prompt-field-label [hint]="hints.exampleDialogues" optional htmlFor="exampleDialogues" />
        <qt-markdown-field
          ariaLabel="Example Dialogues"
          minHeight="12rem"
          [value]="form().exampleDialogues"
          (contentChange)="fieldChange.emit({ ...form(), exampleDialogues: $event })"
        />
      </div>

      <!-- System Prompt -->
      <div>
        <qt-prompt-field-label [hint]="hints.systemPrompt" optional />
        <qt-markdown-field
          ariaLabel="System Prompt"
          minHeight="8rem"
          [value]="form().systemPrompt"
          (contentChange)="fieldChange.emit({ ...form(), systemPrompt: $event })"
        />
      </div>

      <!-- Tags -->
      <qt-tag-chip-editor
        [tagIds]="form().tags"
        (tagIdsChange)="fieldChange.emit({ ...form(), tags: $event })"
      />
    </div>
  `,
})
export class CharacterDetailsTab {
  readonly form = input.required<CharacterFormData>();
  readonly fieldChange = output<CharacterFormData>();

  protected readonly aliasDraft = signal('');
  protected readonly pronounPresets = PRONOUN_PRESETS;
  protected readonly hints = PROMPT_FIELD_HINTS;

  protected readonly pronounPreset = computed(() => getPronounPreset(this.form().pronouns));

  protected readonly coreWhisperSelectValue = computed(() => {
    const v = this.form().coreWhisperEnabled;
    return v === true ? 'on' : v === false ? 'off' : 'inherit';
  });

  protected addAlias(event: Event): void {
    event.preventDefault();
    const trimmed = this.aliasDraft().trim();
    if (trimmed && !this.form().aliases.includes(trimmed)) {
      this.fieldChange.emit({ ...this.form(), aliases: [...this.form().aliases, trimmed] });
    }
    this.aliasDraft.set('');
  }

  protected removeAlias(index: number): void {
    this.fieldChange.emit({
      ...this.form(),
      aliases: this.form().aliases.filter((_, i) => i !== index),
    });
  }

  protected onPronounPresetChange(label: string): void {
    const preset = PRONOUN_PRESETS.find((p) => p.label === label);
    if (!preset) {
      return;
    }
    let pronouns: Pronouns | null;
    if (preset.value === null) {
      pronouns = null;
    } else if (preset.value === 'custom') {
      pronouns = this.form().pronouns ?? { subject: '', object: '', possessive: '' };
    } else {
      pronouns = { ...preset.value };
    }
    this.fieldChange.emit({ ...this.form(), pronouns });
  }

  protected setCustomPronoun(field: keyof Pronouns, value: string): void {
    const current = this.form().pronouns ?? { subject: '', object: '', possessive: '' };
    this.fieldChange.emit({ ...this.form(), pronouns: { ...current, [field]: value } });
  }

  protected onCoreWhisperChange(value: string): void {
    const coreWhisperEnabled = value === 'on' ? true : value === 'off' ? false : null;
    this.fieldChange.emit({ ...this.form(), coreWhisperEnabled });
  }

  protected onScenariosChange(scenarios: CharacterScenario[]): void {
    this.fieldChange.emit({ ...this.form(), scenarios });
  }
}
