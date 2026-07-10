/**
 * Staff / personified-sender display labels — a port of v4
 * `app/salon/[id]/components/system-message-labels.ts` (HEAD a7b1398d). The maps
 * are verbatim; the legacy content-sniffing `inferKindFromContent` fallback (for
 * pre-`systemKind` rows) is NOT ported — a null `systemKind` resolves to `''`
 * (tracked deferral; modern rows always carry `systemKind`).
 */

import type { MessageDto, SystemSender } from '../core/core-contract';

type StaffFields = Pick<MessageDto, 'systemSender' | 'systemKind' | 'content'>;

const SENDER_DISPLAY_NAMES: Record<NonNullable<SystemSender>, string> = {
  lantern: 'The Lantern',
  aurora: 'Aurora',
  librarian: 'The Librarian',
  concierge: 'The Concierge',
  prospero: 'Prospero',
  host: 'The Host',
  commonplaceBook: 'The Commonplace Book',
  ariel: 'Ariel',
  carina: 'Carina',
  suparna: 'Suparṇā',
};

const KIND_DISPLAY_OVERRIDES: Record<string, string> = {
  'project-context': 'project information',
  'project-and-general-context': 'project information and context',
  'general-context': 'general context',
  'connection-profile-change': 'connection change',
  'tool-run': 'tool run',
  'carina-response': 'reference answer',
  'carina-error': 'reference desk',
  'memory-recap': 'memory recap',
  'relevant-memories': 'relevant memories',
  'inter-character-memories': 'inter-character memories',
  'opening-outfit': 'opening outfit',
  'outfit-change': 'outfit change',
  'opened-by-user': 'opened by user',
  'opened-by-character': 'opened by character',
  'deleted-by-user': 'deleted by user',
  'deleted-by-character': 'deleted by character',
  'folder-created-by-user': 'folder created by user',
  'folder-created-by-character': 'folder created by character',
  'folder-deleted-by-user': 'folder deleted by user',
  'folder-deleted-by-character': 'folder deleted by character',
  'created-by-user': 'created by user',
  'created-by-character': 'created by character',
  'edited-by-user': 'edited by user',
  'edited-by-character': 'edited by character',
  'moved-by-user': 'moved by user',
  'moved-by-character': 'moved by character',
  'copied-by-user': 'copied by user',
  'copied-by-character': 'copied by character',
  'blob-written-by-user': 'asset added by user',
  'blob-written-by-character': 'asset added by character',
  'silent-mode-enter': 'silent mode (entering)',
  'silent-mode-exit': 'silent mode (leaving)',
  'user-character': 'user character',
  'character-image': 'character image',
  'join-scenario': 'join scenario',
  'status-change': 'status change',
  'session-opened': 'terminal opened',
  'session-closed': 'terminal closed',
  'autonomous-room-start': 'run begun',
  'autonomous-room-end': 'run ended',
  'autonomous-room-paused': 'run paused',
  'autonomous-room-halfway': 'halfway through',
  'autonomous-room-nearing-end': 'nearing the end',
  'mail-delivery': 'mail delivery',
  'turn-pass': 'nothing to add',
  timestamp: 'time',
};

export function getSystemSenderDisplayName(sender: SystemSender): string {
  if (!sender) return '';
  return SENDER_DISPLAY_NAMES[sender] ?? sender;
}

function resolveRawKind(message: StaffFields): string {
  if (message.systemKind) return message.systemKind;
  return '';
}

export function getSystemKindDisplayLabel(message: StaffFields): string {
  const raw = resolveRawKind(message);
  if (!raw) return '';
  return KIND_DISPLAY_OVERRIDES[raw] ?? raw.replace(/-/g, ' ');
}

export type AnnouncementImportance = 'high' | 'medium' | 'low';

const IMPORTANCE_TABLE: Record<NonNullable<SystemSender>, Record<string, AnnouncementImportance>> = {
  librarian: {
    saved: 'high',
    deleted: 'high',
    renamed: 'high',
    'folder-created': 'high',
    'folder-deleted': 'high',
    attached: 'high',
    summary: 'medium',
    opened: 'low',
    created: 'high',
    'created-by-user': 'high',
    'created-by-character': 'high',
    edited: 'high',
    'edited-by-user': 'high',
    'edited-by-character': 'high',
    moved: 'high',
    'moved-by-user': 'high',
    'moved-by-character': 'high',
    copied: 'high',
    'copied-by-user': 'high',
    'copied-by-character': 'high',
    'blob-written': 'high',
    'blob-written-by-user': 'high',
    'blob-written-by-character': 'high',
    '*': 'medium',
  },
  host: {
    add: 'high',
    remove: 'high',
    'status-change': 'high',
    'user-character': 'high',
    scenario: 'medium',
    roster: 'medium',
    timestamp: 'low',
    'join-scenario': 'low',
    'silent-mode-enter': 'low',
    'silent-mode-exit': 'low',
    'autonomous-room-start': 'medium',
    'autonomous-room-end': 'high',
    'autonomous-room-paused': 'high',
    'autonomous-room-halfway': 'medium',
    'autonomous-room-nearing-end': 'high',
    'turn-pass': 'low',
    '*': 'medium',
  },
  concierge: { danger: 'high', '*': 'high' },
  lantern: { background: 'medium', 'character-image': 'medium', image: 'medium', '*': 'medium' },
  aurora: {
    avatar: 'medium',
    'outfit-change': 'medium',
    'opening-outfit': 'medium',
    wardrobe: 'medium',
    '*': 'medium',
  },
  ariel: { 'session-opened': 'medium', 'session-closed': 'medium', terminal: 'medium', '*': 'medium' },
  prospero: {
    'connection-profile-change': 'medium',
    'project-context': 'low',
    'general-context': 'low',
    'project-and-general-context': 'low',
    announcement: 'low',
    '*': 'low',
  },
  commonplaceBook: {
    'memory-recap': 'low',
    'relevant-memories': 'low',
    'inter-character-memories': 'low',
    consolidated: 'low',
    '*': 'low',
  },
  carina: { 'carina-response': 'medium', '*': 'medium' },
  suparna: { 'mail-delivery': 'high', '*': 'high' },
};

const DEFAULT_IMPORTANCE: AnnouncementImportance = 'medium';

export function getAnnouncementImportance(message: StaffFields): AnnouncementImportance {
  if (!message.systemSender) return DEFAULT_IMPORTANCE;
  const senderTable = IMPORTANCE_TABLE[message.systemSender];
  if (!senderTable) return DEFAULT_IMPORTANCE;
  const kind = resolveRawKind(message);
  return senderTable[kind] ?? senderTable['*'] ?? DEFAULT_IMPORTANCE;
}
