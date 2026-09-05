/**
 * `HelpChatMessageList` — port of v4
 * `components/help-chat/HelpChatMessageList.tsx`.
 *
 * A plain list (no virtualisation — help chats are short), rendering each
 * message through v5's shared markdown renderer, plus the streaming row and the
 * two link strips a help turn can produce: `navigationLinks` from
 * `help_navigate` and `suggestedLinks` from `help_search`.
 *
 * Two v4 filters worth naming, because both look like omissions:
 *
 *  - Only USER and ASSISTANT rows render, and an assistant row with no visible
 *    content is hidden — those are agent-mode's intermediate tool-using turns,
 *    which persist as empty messages.
 *  - A suggested link that duplicates an explicit navigation link is dropped, so
 *    the same page is never offered twice under one answer.
 *
 * @module help/help-message-list
 */

import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  afterRenderEffect,
  computed,
  input,
  output,
  viewChild,
} from '@angular/core';

import { MessageContent } from '../chat/message-content';
import { Icon } from '../ui/icon';
import type { NavigationLink } from './help-stream';
import type { HelpChatMessage } from './help-wire';

/** One help seat, as the list needs it (v4 `CharacterInfo`). */
export interface HelpCharacterInfo {
  id: string;
  name: string;
  avatarUrl: string | null;
}

@Component({
  selector: 'qt-help-message-list',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, MessageContent],
  host: { class: 'contents' },
  template: `
    <div class="flex flex-col gap-3 p-4 overflow-y-auto flex-1">
      @if (visibleMessages().length === 0 && !isStreaming()) {
        <div class="text-center qt-text-secondary text-sm py-8">Ask a question to get started</div>
      }

      @for (msg of visibleMessages(); track msg.id) {
        <div class="flex items-start" [class.flex-row-reverse]="isUser(msg)" [class.flex-row]="!isUser(msg)">
          @if (!isUser(msg)) {
            <div class="qt-help-avatar">
              @if (characterFor(msg); as char) {
                @if (char.avatarUrl) {
                  <img [src]="char.avatarUrl" [alt]="char.name" class="w-full h-full object-cover" />
                } @else {
                  <span class="text-xs qt-text-secondary">{{ char.name[0] || '?' }}</span>
                }
              } @else {
                <span class="text-xs qt-text-secondary">?</span>
              }
            </div>
          }

          <svg
            class="qt-help-tail"
            [class.qt-help-tail-user]="isUser(msg)"
            [class.qt-help-tail-assistant]="!isUser(msg)"
            viewBox="0 0 10 16"
            fill="currentColor"
          >
            @if (isUser(msg)) {
              <path d="M0 0 L10 8 L0 16 Z" />
            } @else {
              <path d="M10 0 L0 8 L10 16 Z" />
            }
          </svg>

          <div [class]="isUser(msg) ? 'qt-help-msg-user' : 'qt-help-msg-assistant'">
            @if (!isUser(msg) && characterFor(msg); as char) {
              <div class="qt-help-msg-character-name">{{ char.name }}</div>
            }
            <qt-message-content [content]="msg.content" />
          </div>
        </div>
      }

      @if (isStreaming() && streamingContent()) {
        <div class="flex items-start flex-row">
          <div class="qt-help-avatar">
            @if (streamingCharacter(); as char) {
              @if (char.avatarUrl) {
                <img [src]="char.avatarUrl" [alt]="char.name" class="w-full h-full object-cover" />
              } @else {
                <span class="text-xs qt-text-secondary">{{ char.name[0] || '?' }}</span>
              }
            } @else {
              <span class="text-xs qt-text-secondary">?</span>
            }
          </div>
          <svg class="qt-help-tail qt-help-tail-assistant" viewBox="0 0 10 16" fill="currentColor">
            <path d="M10 0 L0 8 L10 16 Z" />
          </svg>
          <div class="qt-help-msg-assistant">
            <qt-message-content [content]="streamingContent()" />
          </div>
        </div>
      }

      @if (isStreaming() && !streamingContent()) {
        <div class="flex items-start flex-row">
          <div class="qt-help-avatar">
            @if (streamingCharacter(); as char) {
              @if (char.avatarUrl) {
                <img [src]="char.avatarUrl" [alt]="char.name" class="w-full h-full object-cover" />
              } @else {
                <span class="text-xs qt-text-secondary">{{ char.name[0] || '...' }}</span>
              }
            } @else {
              <span class="text-xs qt-text-secondary">...</span>
            }
          </div>
          <svg class="qt-help-tail qt-help-tail-assistant" viewBox="0 0 10 16" fill="currentColor">
            <path d="M10 0 L0 8 L10 16 Z" />
          </svg>
          <div class="qt-help-msg-assistant italic">
            {{ isExecutingTools() ? 'Consulting the archives...' : 'Thinking...' }}
          </div>
        </div>
      }

      @if (showNavLinks()) {
        <div class="flex flex-wrap gap-2 pl-10">
          @for (link of navigationLinks(); track link.url) {
            <button type="button" class="qt-help-nav-button" (click)="navigate.emit(link.url)">
              <qt-icon name="external-link" class="w-3.5 h-3.5 flex-shrink-0" />
              {{ link.label }}
            </button>
          }
        </div>
      }

      @if (showSuggestions()) {
        <div class="qt-help-suggested-links">
          <div class="qt-help-suggested-links-label">Related pages</div>
          <div class="flex flex-wrap gap-1.5">
            @for (link of filteredSuggestions(); track link.url) {
              <button
                type="button"
                class="qt-help-suggested-link"
                (click)="navigate.emit(link.url)"
              >
                <qt-icon name="chevron-right" class="w-3 h-3 flex-shrink-0" />
                {{ link.label }}
              </button>
            }
          </div>
        </div>
      }

      <div #end></div>
    </div>
  `,
})
export class HelpMessageList {
  readonly messages = input.required<HelpChatMessage[]>();
  readonly characterMap = input.required<Map<string, HelpCharacterInfo>>();
  readonly participantToCharacter = input.required<Map<string, string>>();
  readonly streamingContent = input('');
  readonly streamingParticipantId = input<string | null>(null);
  readonly isStreaming = input(false);
  readonly isExecutingTools = input(false);
  readonly navigationLinks = input<NavigationLink[]>([]);
  /** Links extracted from help_search results — suggested pages by relevance. */
  readonly suggestedLinks = input<NavigationLink[]>([]);
  readonly navigate = output<string>();

  private readonly endEl = viewChild.required<ElementRef<HTMLDivElement>>('end');

  /**
   * User + assistant only, and never an assistant turn with no visible content:
   * those are agent mode's intermediate tool-using iterations, which persist as
   * empty messages (v4's comment, kept because the filter reads like a bug).
   */
  protected readonly visibleMessages = computed(() =>
    this.messages().filter((m) => {
      if (m.role === 'USER' || m.role === 'user') return true;
      if (m.role === 'ASSISTANT' || m.role === 'assistant') {
        return !!m.content && m.content.trim().length > 0;
      }
      return false;
    }),
  );

  protected readonly showNavLinks = computed(
    () => this.navigationLinks().length > 0 && !this.isStreaming(),
  );

  /** Suggestions minus anything already offered as an explicit nav link. */
  protected readonly filteredSuggestions = computed(() => {
    const navUrls = new Set(this.navigationLinks().map((l) => l.url));
    return this.suggestedLinks().filter((l) => !navUrls.has(l.url));
  });

  protected readonly showSuggestions = computed(
    () => this.filteredSuggestions().length > 0 && !this.isStreaming(),
  );

  protected readonly streamingCharacter = computed(() =>
    this.lookup(this.streamingParticipantId()),
  );

  constructor() {
    // Auto-scroll on new messages or streaming (v4's effect).
    afterRenderEffect(() => {
      void this.visibleMessages().length;
      void this.streamingContent();
      void this.navigationLinks().length;
      void this.suggestedLinks().length;
      this.endEl().nativeElement.scrollIntoView({ behavior: 'smooth' });
    });
  }

  protected isUser(msg: HelpChatMessage): boolean {
    return msg.role === 'USER' || msg.role === 'user';
  }

  protected characterFor(msg: HelpChatMessage): HelpCharacterInfo | null {
    return this.lookup(msg.participantId);
  }

  private lookup(participantId: string | null | undefined): HelpCharacterInfo | null {
    if (!participantId) return null;
    const charId = this.participantToCharacter().get(participantId);
    if (!charId) return null;
    return this.characterMap().get(charId) || null;
  }
}
