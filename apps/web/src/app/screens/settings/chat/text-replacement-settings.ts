import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';

import { CoreClient } from '../../../core/core-client';
import type { TextReplacementRule } from '../../../core/core-contract';
import { compileRules, findReplacement, TRIGGER_CHARS } from '../../../editor/text-replacement';
import {
  createTextReplacement,
  deleteTextReplacement,
  listTextReplacements,
  updateTextReplacement,
} from './text-replacements.api';

const isBoundaryChar = (ch: string): boolean => TRIGGER_CHARS.has(ch);

/**
 * The Text Replacement settings card (v4
 * `components/settings/chat-settings/TextReplacementSettings.tsx`): a master
 * on/off toggle (`chat_settings.textReplacementsEnabled`) over a CRUD editor for
 * the global rule list, plus a "Try it" box that previews the composer plugin's
 * exact trigger behavior. Copy carries over verbatim.
 *
 * In-lane the CoreClient is mocked; the live rule list + settings edge activate
 * at unification. The rules feed the composer's `TextReplacementPlugin` (§3).
 */
@Component({
  selector: 'qt-text-replacement-settings',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="space-y-4">
      <!-- Master toggle -->
      <label class="qt-settings-toggle-row">
        <input
          type="checkbox"
          class="qt-checkbox mt-1"
          [checked]="enabled()"
          [disabled]="savingToggle()"
          (change)="onToggle($any($event.target).checked)"
        />
        <div class="flex-1">
          <div class="qt-settings-section-heading">Text replacement (autocorrect)</div>
          <div class="qt-text-small mt-1">
            Replaces literal triggers with replacement text on word boundaries (space, tab, and the
            usual terminal punctuation) as you type in the Salon composer and the Document Mode rich
            editor. Pure literal matching — no snippets, no regex. One Cmd/Ctrl+Z reverts a
            replacement. Source-mode editors (raw Markdown, plain text) are unaffected.
          </div>
        </div>
      </label>

      <!-- Add a rule -->
      <form
        class="qt-settings-shell space-y-3"
        aria-label="Add a text-replacement rule"
        (submit)="onAdd($event)"
      >
        <div class="qt-settings-section-heading">Add a rule</div>
        <div class="grid grid-cols-1 sm:grid-cols-[1fr,1fr,auto] gap-2 items-end">
          <label class="flex flex-col gap-1">
            <span class="qt-text-small text-muted-foreground">Trigger</span>
            <input
              type="text"
              class="qt-input"
              maxlength="100"
              placeholder="e.g. teh"
              [value]="newFrom()"
              [disabled]="busy()"
              (input)="newFrom.set($any($event.target).value)"
            />
          </label>
          <label class="flex flex-col gap-1">
            <span class="qt-text-small text-muted-foreground">Replacement</span>
            <input
              type="text"
              class="qt-input"
              maxlength="1000"
              placeholder="e.g. the"
              [value]="newTo()"
              [disabled]="busy()"
              (input)="newTo.set($any($event.target).value)"
            />
          </label>
          <button
            type="submit"
            class="qt-button-primary self-end"
            [disabled]="busy() || !newFrom().trim() || !newTo()"
          >
            Add
          </button>
        </div>
        <label class="flex items-center gap-2 qt-text-small">
          <input
            type="checkbox"
            class="qt-checkbox"
            [checked]="newCaseSensitive()"
            [disabled]="busy()"
            (change)="newCaseSensitive.set($any($event.target).checked)"
          />
          <span>Case-sensitive</span>
        </label>
        @if (addError()) {
          <div class="qt-text-small text-destructive">{{ addError() }}</div>
        }
      </form>

      <!-- Existing rules -->
      <div class="qt-settings-shell p-0">
        <div class="p-4 border-b qt-border-default">
          <div class="qt-settings-section-heading">Rules</div>
          <div class="qt-text-small text-muted-foreground">
            @if (loading()) {
              Loading…
            } @else {
              {{ rules().length }} rule{{ rules().length === 1 ? '' : 's' }} defined.
            }
          </div>
        </div>
        @if (rules().length > 0) {
          <ul>
            @for (rule of rules(); track rule.id) {
              <li
                class="grid grid-cols-1 sm:grid-cols-[1fr,1fr,auto,auto,auto] gap-2 items-center p-3 border-t qt-border-default first:border-t-0"
              >
                <input
                  type="text"
                  class="qt-input"
                  maxlength="100"
                  [value]="rule.fromText"
                  [disabled]="busy()"
                  (blur)="commitFrom(rule, $any($event.target).value)"
                  (keydown.enter)="$any($event.target).blur()"
                />
                <input
                  type="text"
                  class="qt-input"
                  maxlength="1000"
                  [value]="rule.toText"
                  [disabled]="busy()"
                  (blur)="commitTo(rule, $any($event.target).value)"
                  (keydown.enter)="$any($event.target).blur()"
                />
                <label class="flex items-center gap-1 qt-text-small whitespace-nowrap">
                  <input
                    type="checkbox"
                    class="qt-checkbox"
                    [checked]="rule.caseSensitive"
                    [disabled]="busy()"
                    (change)="patch(rule, { caseSensitive: $any($event.target).checked })"
                  />
                  <span>Case</span>
                </label>
                <label class="flex items-center gap-1 qt-text-small whitespace-nowrap">
                  <input
                    type="checkbox"
                    class="qt-checkbox"
                    [checked]="rule.enabled"
                    [disabled]="busy()"
                    (change)="patch(rule, { enabled: $any($event.target).checked })"
                  />
                  <span>On</span>
                </label>
                <button
                  type="button"
                  class="qt-button-secondary"
                  [disabled]="busy()"
                  [attr.aria-label]="'Delete rule for &quot;' + rule.fromText + '&quot;'"
                  (click)="remove(rule)"
                >
                  Delete
                </button>
              </li>
            }
          </ul>
        } @else if (!loading()) {
          <div class="p-4 qt-text-small text-muted-foreground italic">
            No rules yet — add one above.
          </div>
        }
      </div>

      <!-- Try it -->
      <div class="qt-settings-shell qt-settings-field-group">
        <div class="qt-settings-section-heading">Try it</div>
        <div class="qt-text-small text-muted-foreground">
          Type here and add a trigger character (space, period, etc.) to see your rules fire. This
          box does not save anything.
          @if (!enabled()) {
            (Master toggle is off — replacements are paused.)
          }
        </div>
        <textarea
          rows="3"
          class="qt-input w-full"
          aria-label="Try text replacement"
          placeholder="Type a trigger word, then press space…"
          [value]="tryText()"
          (input)="tryText.set($any($event.target).value)"
          (keydown)="onTryKeydown($event)"
        ></textarea>
      </div>
    </div>
  `,
})
export class TextReplacementSettings {
  private readonly core = inject(CoreClient);

  protected readonly loading = signal(true);
  protected readonly busy = signal(false);
  protected readonly savingToggle = signal(false);
  protected readonly rules = signal<TextReplacementRule[]>([]);
  protected readonly enabled = signal(true);

  protected readonly newFrom = signal('');
  protected readonly newTo = signal('');
  protected readonly newCaseSensitive = signal(false);
  protected readonly addError = signal<string | null>(null);

  protected readonly tryText = signal('');

  /** The plugin's compiled maps — the "Try it" box previews the real behavior. */
  private readonly compiled = computed(() => compileRules(this.rules()));

  constructor() {
    void this.load();
  }

  private async load(): Promise<void> {
    try {
      const [list, settings] = await Promise.all([
        listTextReplacements(this.core),
        this.core.dispatchExpect({ type: 'chatSettings' }, 'chatSettings'),
      ]);
      this.rules.set(list.rules);
      this.enabled.set(settings.data['textReplacementsEnabled'] !== false);
    } catch {
      // leave defaults; the surface degrades to empty
    } finally {
      this.loading.set(false);
    }
  }

  private async reload(): Promise<void> {
    try {
      const list = await listTextReplacements(this.core);
      this.rules.set(list.rules);
    } catch {
      /* keep the current list */
    }
  }

  protected async onToggle(value: boolean): Promise<void> {
    this.savingToggle.set(true);
    const previous = this.enabled();
    this.enabled.set(value);
    try {
      await this.core.dispatchExpect(
        { type: 'chatSettingsUpdate', settings: { textReplacementsEnabled: value } },
        'chatSettings',
      );
    } catch {
      this.enabled.set(previous);
    } finally {
      this.savingToggle.set(false);
    }
  }

  protected async onAdd(event: Event): Promise<void> {
    event.preventDefault();
    this.addError.set(null);
    const fromText = this.newFrom().trim();
    const toText = this.newTo();
    if (!fromText || !toText) {
      this.addError.set('Both trigger and replacement are required.');
      return;
    }
    this.busy.set(true);
    try {
      await createTextReplacement(this.core, {
        fromText,
        toText,
        caseSensitive: this.newCaseSensitive(),
      });
      this.newFrom.set('');
      this.newTo.set('');
      this.newCaseSensitive.set(false);
      await this.reload();
    } catch (err) {
      this.addError.set(err instanceof Error && err.message ? err.message : 'Failed to add rule.');
    } finally {
      this.busy.set(false);
    }
  }

  protected commitFrom(rule: TextReplacementRule, value: string): void {
    const next = value.trim();
    if (next && next !== rule.fromText) void this.patch(rule, { fromText: next });
  }

  protected commitTo(rule: TextReplacementRule, value: string): void {
    if (value && value !== rule.toText) void this.patch(rule, { toText: value });
  }

  protected async patch(
    rule: TextReplacementRule,
    partial: Partial<Pick<TextReplacementRule, 'fromText' | 'toText' | 'caseSensitive' | 'enabled'>>,
  ): Promise<void> {
    this.busy.set(true);
    try {
      await updateTextReplacement(this.core, rule.id, partial);
    } catch {
      /* fall through to the reload, which restores server truth */
    } finally {
      await this.reload();
      this.busy.set(false);
    }
  }

  protected async remove(rule: TextReplacementRule): Promise<void> {
    this.busy.set(true);
    try {
      await deleteTextReplacement(this.core, rule.id);
    } catch {
      /* keep the row; the reload restores truth */
    } finally {
      await this.reload();
      this.busy.set(false);
    }
  }

  /** The "Try it" preview — the plugin's exact trigger logic on a textarea. */
  protected onTryKeydown(event: KeyboardEvent): void {
    if (!this.enabled()) return;
    const compiled = this.compiled();
    if (compiled.empty) return;
    if (event.isComposing) return;
    if (!TRIGGER_CHARS.has(event.key)) return;

    const ta = event.target as HTMLTextAreaElement;
    const start = ta.selectionStart;
    const end = ta.selectionEnd;
    if (start !== end) return;

    const value = ta.value;
    const nextChar = value[start];
    if (nextChar !== undefined && !isBoundaryChar(nextChar)) return;

    let wordStart = start;
    while (wordStart > 0 && !isBoundaryChar(value[wordStart - 1])) wordStart--;
    if (wordStart === start) return;

    const word = value.slice(wordStart, start);
    const replacement = findReplacement(word, compiled);
    if (replacement === undefined) return;

    event.preventDefault();
    const triggerChar = event.key;
    const newValue = value.slice(0, wordStart) + replacement + triggerChar + value.slice(start);
    const cursor = wordStart + replacement.length + triggerChar.length;
    this.tryText.set(newValue);
    // Restore the caret after the value re-render (v4's pending-cursor ref).
    queueMicrotask(() => {
      ta.value = newValue;
      ta.selectionStart = cursor;
      ta.selectionEnd = cursor;
    });
  }
}
