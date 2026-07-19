import {
  ChangeDetectionStrategy,
  Component,
  OnInit,
  computed,
  inject,
  signal,
} from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute } from '@angular/router';

import { AutonomousRoomDefaults } from '../../../autonomous/autonomous-room-defaults';
import { AutonomousRoomsList } from '../../../autonomous/autonomous-rooms-list';
import { CollapsibleCard } from '../../../ui/collapsible-card';
import { AgentModeSettings } from './agent-mode-settings';
import { AnswerConfirmationSettings } from './answer-confirmation-settings';
import { AutoScrollSettings } from './auto-scroll-settings';
import { AutomationSettings } from './automation-settings';
import { ComposerSpellcheckSettings } from './composer-spellcheck-settings';
import { CompositionModeSettings } from './composition-mode-settings';
import { ContextCompressionSettings } from './context-compression-settings';
import { CustomToolsSettings } from './custom-tools-settings';
import { DangerousContentSettings } from './dangerous-content-settings';
import { DataRetentionSettings } from './data-retention-settings';
import { ImageDescriptionSettings } from './image-description-settings';
import { MemoryCascadeSettings } from './memory-cascade-settings';
import { TextReplacementSettings } from './text-replacement-settings';
import { ThinkingDisplaySettings } from './thinking-display-settings';
import { TokenDisplaySettings } from './token-display-settings';

/**
 * The Settings → Chat tab (v4 `components/settings/tabs/ChatTabContent.tsx`,
 * subsystem `salon`) — now FULLY fitted out: all seventeen v4 cards, in v4's
 * exact order, with v4's titles/descriptions + `sectionId`s (the `?section=`
 * deep link) ported verbatim.
 *
 * The order is v4's, and is reproduced rather than tidied: Composer and
 * Auto-Scroll sit BETWEEN Composition Mode and Text Replacement, Custom Tools
 * sits BETWEEN Automation and Agent Mode, and the engine-facing cards sit
 * between Text Replacement and Data Retention. Every card reads and writes the
 * one `chat_settings` row through the shared `chatSettings` query
 * (`chat-settings.api.ts`), so mounting seventeen of them costs ONE GET.
 *
 * Cards by lineage: Composition Mode + Text Replacement (P4.6al), Data
 * Retention (P4.d3), the two autonomous cards (P4.6ad), the eleven landed by
 * P4.6an, and Custom Tools (P4.6ba — the Workbench button v4 pairs with it is
 * deferred to P4.6bb).
 */
@Component({
  selector: 'qt-settings-chat',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    CollapsibleCard,
    AutonomousRoomDefaults,
    AutonomousRoomsList,
    AgentModeSettings,
    AnswerConfirmationSettings,
    AutoScrollSettings,
    AutomationSettings,
    ComposerSpellcheckSettings,
    CompositionModeSettings,
    ContextCompressionSettings,
    CustomToolsSettings,
    DangerousContentSettings,
    DataRetentionSettings,
    ImageDescriptionSettings,
    MemoryCascadeSettings,
    TextReplacementSettings,
    ThinkingDisplaySettings,
    TokenDisplaySettings,
  ],
  template: `
    <div>
      <p class="qt-text-small qt-text-muted italic mb-6">
        The Salon's standing house rules — how conversations compose themselves, keep their books,
        and, of late, run on without you.
      </p>

      <div class="space-y-4">
        <qt-collapsible-card
          title="Composition Mode"
          description="Whether new chats start in composition mode"
          sectionId="composition-mode"
          [defaultOpen]="defaultOpen()"
          [forceOpen]="section() === 'composition-mode'"
        >
          <qt-composition-mode-settings />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Composer"
          description="Composer behavior and input aids"
          sectionId="composer-spellcheck"
          [forceOpen]="section() === 'composer-spellcheck'"
        >
          <qt-composer-spellcheck-settings />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Auto-Scroll"
          description="Whether the Salon follows new messages to the bottom"
          sectionId="auto-scroll"
          [forceOpen]="section() === 'auto-scroll'"
        >
          <qt-auto-scroll-settings />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Text Replacement"
          description="Replace literal triggers with replacement text on word boundaries"
          sectionId="text-replacements"
          [forceOpen]="section() === 'text-replacements'"
        >
          <qt-text-replacement-settings />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Token Display"
          description="Configure token count and cost display"
          sectionId="token-display"
          [forceOpen]="section() === 'token-display'"
        >
          <qt-token-display-settings />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Context Compression"
          description="Configure how older messages are compressed"
          sectionId="context-compression"
          [forceOpen]="section() === 'context-compression'"
        >
          <qt-context-compression-settings />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Memory Cascade"
          description="Control how memories behave when messages change"
          sectionId="memory-cascade"
          [forceOpen]="section() === 'memory-cascade'"
        >
          <qt-memory-cascade-settings />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Image Description"
          description="Configure image description generation"
          sectionId="image-description"
          [forceOpen]="section() === 'image-description'"
        >
          <qt-image-description-settings />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Automation"
          description="Configure automatic detection features"
          sectionId="automation"
          [forceOpen]="section() === 'automation'"
        >
          <qt-automation-settings />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Custom Tools"
          description="Whether Pascal's custom tools are offered to models and the composer"
          sectionId="custom-tools"
          [forceOpen]="section() === 'custom-tools'"
        >
          <qt-custom-tools-settings />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Agent Mode"
          description="Configure iterative tool use with self-correction"
          sectionId="agent-mode"
          [forceOpen]="section() === 'agent-mode'"
        >
          <qt-agent-mode-settings />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Thinking / Reasoning"
          description="Show reasoning models' chain-of-thought in chat (display only)"
          sectionId="thinking-display"
          [forceOpen]="section() === 'thinking-display'"
        >
          <qt-thinking-display-settings />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Answer Confirmation"
          description="Vet looked-up answers against what the character actually knew this turn"
          sectionId="answer-confirmation"
          [forceOpen]="section() === 'answer-confirmation'"
        >
          <qt-answer-confirmation-settings />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Dangerous Content"
          description="Configure content detection, routing, and display behavior"
          sectionId="dangerous-content"
          [forceOpen]="section() === 'dangerous-content'"
        >
          <qt-dangerous-content-settings />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Data Retention"
          description="How long inactive chats keep their regenerable working data"
          sectionId="data-retention"
          [forceOpen]="section() === 'data-retention'"
        >
          <qt-data-retention-settings />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Autonomous Rooms"
          description="Defaults for private character-to-character rooms"
          sectionId="autonomous-rooms"
          [forceOpen]="section() === 'autonomous-rooms'"
        >
          <qt-autonomous-room-defaults />
        </qt-collapsible-card>

        <qt-collapsible-card
          title="Scheduled Autonomous Rooms"
          description="Pause, resume, or stop scheduled rooms (and any ad-hoc room currently running)"
          sectionId="autonomous-room-schedules"
          [forceOpen]="section() === 'autonomous-room-schedules'"
        >
          <qt-autonomous-rooms-list />
        </qt-collapsible-card>
      </div>
    </div>
  `,
})
export class ChatTab implements OnInit {
  // Optional so the tab renders when hosted as a workspace tab (no ActivatedRoute);
  // v4 ignores `?section=` when hosted, so the deep-link simply falls back to null.
  private readonly route = inject(ActivatedRoute, { optional: true });
  private readonly queryParams = this.route
    ? toSignal(this.route.queryParamMap, { requireSync: true })
    : undefined;

  protected readonly section = computed(() => this.queryParams?.().get('section') ?? null);
  protected readonly defaultOpen = signal(false);

  ngOnInit(): void {
    // Open the first card by default (bound inputs have resolved by ngOnInit).
    this.defaultOpen.set(true);
  }
}
