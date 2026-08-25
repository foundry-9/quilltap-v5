import { ChangeDetectionStrategy, Component, computed, input, signal } from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { MarkdownField } from '../editor/markdown-field';

/** Loads the field's markdown (v4's GET on `loadUrl`, returning `{content}`). */
export type AestheticLoadFn = () => Promise<string>;
/** Saves it back (v4's PUT of `{content}`); empty content deletes the file. */
export type AestheticSaveFn = (content: string) => Promise<void>;

/**
 * `qt-aesthetic-editor-field` — a markdown editor wired to a document-store-backed
 * aesthetic file (v4 `components/settings/AestheticEditorField.tsx`). v4 has ONE
 * such component serving three surfaces; this is its port:
 *
 *   - the Images settings tab  → `systemImageAestheticsGet/Set`
 *   - the project image card   → `projectAestheticGet/Set`
 *   - (v4 also: the character edit page's depiction guidelines)
 *
 * v4 addresses the endpoint with `loadUrl`/`saveUrl`; over the dispatch boundary
 * there is no URL to pass, so the host injects {@link load} / {@link save}
 * callbacks and the TanStack {@link queryKey} instead. Everything else is v4's:
 * the `8rem` editor (`:119`), `ariaLabel={label}` (`:120`), the primary
 * `Saving…`/`Save` button disabled on `saving || !dirty` (`:122-130`), the
 * `saved && !dirty` success span (`:131`), and the shared error span (`:132`)
 * carrying whichever of the load or save failed.
 *
 * **The load gate is load-bearing, not cosmetic** (v4 `:117-119`, `loading ?
 * 'Loading…' : <editor>`). v4 never emits on a load: its mount-time parse is
 * tagged `external-sync` and its update listener skips that tag
 * (`MarkdownBridgePlugin.tsx:168-181`), which is why v4 can bump `remountKey`
 * after each load without dirtying the form. v5's equivalent is the field's
 * absorb-once seam, and that swallows exactly ONE emit — so the content must be
 * in hand BEFORE the editor mounts. Were the editor to mount empty and take the
 * content later, the load would surface as an edit: the draft would become the
 * editor's canonical re-serialization (`__bold__` → `**bold**`, trailing newline
 * dropped) and the field would go dirty, so a Save the user never meant to make
 * would rewrite their stored file. Seeding inside `queryFn` guarantees the
 * ordering; v4's `dirty` gate is the second belt.
 *
 * NOT ported: v4's `disabledHint` prop (`:29`, `:112`), which suppresses the
 * editor behind a warning. Its only v4 caller is the depiction-guidelines field
 * (`CharacterEditView.tsx:338-345`, for a character with no document vault yet)
 * — and in v5 that field does NOT go through this component: it landed as inline
 * `qt-markdown-field`s on the two characters appearance tabs
 * (`screens/characters/edit/appearance-tab.ts`,
 * `screens/characters/view/tabs/character-appearance-tab.ts`), which is where
 * P4.6aw ported the `disabledHint` behavior itself. So the prop stays unported
 * HERE: neither aesthetic consumer passes it, and no v5 consumer would — it
 * would be dead surface. Should an aesthetic field ever need suppressing, lift
 * the arm from those two tabs.
 */
@Component({
  selector: 'qt-aesthetic-editor-field',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [MarkdownField],
  template: `
    <div class="space-y-2">
      <div>
        <label class="qt-label">{{ label() }}</label>
        @if (description()) {
          <p class="qt-text-small qt-text-muted">{{ description() }}</p>
        }
      </div>

      @if (loadQuery.isPending()) {
        <div class="qt-text-secondary qt-text-small py-4">Loading…</div>
      } @else {
        <qt-markdown-field
          minHeight="8rem"
          [ariaLabel]="label()"
          [value]="content()"
          (contentChange)="onContentChange($event)"
        />
        <div class="flex items-center gap-3">
          <button
            type="button"
            class="qt-button qt-button-primary qt-button-sm"
            [disabled]="saving() || !dirty()"
            (click)="onSave()"
          >
            {{ saving() ? 'Saving…' : 'Save' }}
          </button>
          @if (saved() && !dirty()) {
            <span class="qt-text-small qt-text-success">Saved</span>
          }
          @if (error(); as msg) {
            <span class="qt-text-small qt-text-destructive">{{ msg }}</span>
          }
        </div>
      }
    </div>
  `,
})
export class AestheticEditorField {
  readonly label = input('');
  readonly description = input('');
  /** The TanStack key for this field's content (v4 keys off `loadUrl`). */
  readonly queryKey = input.required<readonly unknown[]>();
  readonly load = input.required<AestheticLoadFn>();
  readonly save = input.required<AestheticSaveFn>();

  protected readonly content = signal('');
  protected readonly saving = signal(false);
  protected readonly saved = signal(false);
  protected readonly dirty = signal(false);
  protected readonly saveError = signal<string | null>(null);

  // Seeded from inside queryFn, so `content` is already set the moment the query
  // leaves the pending state and the editor mounts (see the class doc — the
  // editor must never mount ahead of its content).
  protected readonly loadQuery = injectQuery(() => ({
    queryKey: this.queryKey(),
    queryFn: async (): Promise<string> => {
      const loaded = await this.load()();
      this.content.set(loaded);
      return loaded;
    },
  }));

  /**
   * v4 keeps ONE `error` for both legs (`:43`): a load failure sets it, a save
   * clears it and may set its own. A failed load still renders the editor (v4's
   * `finally { setLoading(false) }`, `:70-72`) — empty and pristine, so the
   * dirty gate keeps it from writing that emptiness back.
   */
  protected readonly error = computed(() => {
    const save = this.saveError();
    if (save !== null) return save;
    const err = this.loadQuery.error();
    if (!err) return null;
    return err instanceof Error ? err.message : 'Failed to load';
  });

  protected onContentChange(value: string): void {
    this.content.set(value);
    this.dirty.set(true);
    this.saved.set(false);
  }

  protected async onSave(): Promise<void> {
    this.saving.set(true);
    this.saveError.set(null);
    try {
      await this.save()(this.content());
      this.dirty.set(false);
      this.saved.set(true);
    } catch (err) {
      this.saveError.set(err instanceof Error ? err.message : 'Failed to save');
    } finally {
      this.saving.set(false);
    }
  }
}
