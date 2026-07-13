import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
  viewChild,
  ElementRef,
} from '@angular/core';

import { CoreClient } from '../core/core-client';
import type { ParticipantDetail } from '../core/core-contract';
import { Modal } from '../ui/modal';

/** A generated image (v4 `?action=generate` `data[]`). */
export interface GeneratedImage {
  id: string;
  filename: string;
  filepath: string;
  mimeType: string;
}

/**
 * GenerateImageDialog (chat mode) — a port of v4
 * `components/chat/GenerateImageDialog.tsx`. Composes a prompt (with
 * `{{Character}}` placeholder quick-inserts for the chat's characters), then
 * generates against the chat's image profile via `imageProfileGenerate`. On
 * success it emits the images + expanded prompt (the conversation records a
 * `chatAddToolResult`). While lane A's generate arm is still refusal-armed the
 * dialog degrades loudly (the `not_available` message in v4 voice).
 *
 * Deferred (loud): the full all-characters entity dropdown (v4 loads
 * `/characters`), the StandaloneGenerateImageDialog + ImageProfilePicker (the
 * explicit-profile / non-chat path), and auto-attaching the generated images to
 * the next message — this dialog records the tool result and refetches.
 */
@Component({
  selector: 'qt-generate-image-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  template: `
    <qt-modal title="Generate Image" maxWidth="3xl" (close)="close.emit()">
      <div class="space-y-4">
        @if (!imageProfileId()) {
          <div class="qt-alert-error text-sm" role="alert">
            No image profile is configured for this chat. Configure an image profile for one of the
            chat's participants to conjure a picture.
          </div>
        }

        @if (characterNames().length > 0) {
          <div>
            <label class="qt-label mb-2 block">Insert a character</label>
            <div class="flex flex-wrap gap-2">
              @for (name of characterNames(); track name) {
                <button
                  type="button"
                  class="qt-button-secondary qt-button-sm"
                  [title]="'Insert {{' + name + '}}'"
                  (click)="insertPlaceholder(name)"
                >
                  {{ name }}
                </button>
              }
            </div>
          </div>
        }

        <div>
          <label for="generate-prompt" class="qt-label mb-2 block">Prompt</label>
          <textarea
            #promptRef
            id="generate-prompt"
            class="qt-input w-full"
            rows="5"
            placeholder="Describe the picture — use {{ '{{' }}Character{{ '}}' }} to weave in a likeness…"
            [value]="prompt()"
            (input)="prompt.set($any($event.target).value)"
          ></textarea>
        </div>

        @if (error(); as e) {
          <div class="qt-alert-error text-sm" role="alert">{{ e }}</div>
        }
      </div>

      <div qt-modal-footer class="flex justify-end gap-2">
        <button type="button" class="qt-button-secondary qt-button-sm" (click)="close.emit()">
          Cancel
        </button>
        <button
          type="button"
          class="qt-button-primary qt-button-sm"
          [disabled]="!canGenerate()"
          (click)="onGenerate()"
        >
          {{ generating() ? 'Conjuring…' : 'Generate' }}
        </button>
      </div>
    </qt-modal>
  `,
})
export class GenerateImageDialog {
  private readonly core = inject(CoreClient);

  readonly chatId = input.required<string>();
  readonly imageProfileId = input<string | null>(null);
  readonly participants = input<ParticipantDetail[]>([]);

  readonly close = output<void>();
  readonly generated = output<{ images: GeneratedImage[]; prompt: string }>();

  private readonly promptRef = viewChild<ElementRef<HTMLTextAreaElement>>('promptRef');

  protected readonly prompt = signal('');
  protected readonly generating = signal(false);
  protected readonly error = signal<string | null>(null);

  /** The chat's character names (v4's quick-insert buttons; user char included). */
  protected readonly characterNames = computed(() =>
    this.participants()
      .filter((p) => p.type === 'CHARACTER' && p.character)
      .map((p) => p.character!.name),
  );

  protected readonly canGenerate = computed(
    () => !this.generating() && !!this.imageProfileId() && this.prompt().trim().length > 0,
  );

  protected insertPlaceholder(name: string): void {
    const textarea = this.promptRef()?.nativeElement;
    const placeholder = `{{${name}}}`;
    if (!textarea) {
      this.prompt.update((p) => p + placeholder);
      return;
    }
    const start = textarea.selectionStart;
    const end = textarea.selectionEnd;
    const current = this.prompt();
    const next = current.substring(0, start) + placeholder + current.substring(end);
    this.prompt.set(next);
    setTimeout(() => {
      textarea.focus();
      const pos = start + placeholder.length;
      textarea.setSelectionRange(pos, pos);
    }, 0);
  }

  protected async onGenerate(): Promise<void> {
    const imageProfileId = this.imageProfileId();
    const prompt = this.prompt();
    if (!imageProfileId || !prompt.trim()) return;
    this.generating.set(true);
    this.error.set(null);
    try {
      const resp = await this.core.dispatch({
        type: 'imageProfileGenerate',
        imageProfileId,
        prompt,
        chatId: this.chatId(),
        count: 1,
      });
      if (resp.type === 'error') {
        throw new Error(resp.data.message);
      }
      const body = (resp.data ?? {}) as {
        success?: boolean;
        data?: GeneratedImage[];
        expandedPrompt?: string;
      };
      if (body.success && body.data && body.data.length > 0) {
        this.generated.emit({ images: body.data, prompt: body.expandedPrompt || prompt });
        this.close.emit();
      } else {
        this.error.set('No images were generated.');
      }
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : 'Failed to generate image.');
    } finally {
      this.generating.set(false);
    }
  }
}
