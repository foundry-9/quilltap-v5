/**
 * Default titles and icons per tab kind (port of v4 `lib/workspace/tab-meta.ts`,
 * baseline `b8b12695`).
 *
 * Used by the reducer when it mints a tab on its own (the home fallback) and by
 * the store's `openTab` convenience when a caller supplies no explicit
 * title/icon. User-facing titles are in the house voice.
 *
 * v5-only kinds (today: `salon-new`) are marked inline; the corpus assertion
 * compares only the keys v4 declares.
 *
 * The icon values are v4's canonical icon names. Every one of them exists
 * verbatim in v5's `ui/icon.ts` registry (all 21 map 1:1), so the table is kept
 * byte-for-byte — the tier-1 corpus diffs this table against v4's. Were a v4
 * icon name ever to lack a v5 equivalent, the v4 NAME would stay here (the
 * corpus is the oracle) and the nearest v5 icon would be mapped at the render
 * site; today no such remap is needed.
 *
 * @module workspace/core/tab-meta
 */

import type { TabKind } from '../workspace-contract';

export interface TabMeta {
  title: string;
  icon: string;
}

/**
 * Per-kind defaults. Salon/terminal/document/settings/wardrobe titles are
 * usually overridden by the opener with a chat- or target-specific label; the
 * defaults here are the fallbacks.
 */
export const DEFAULT_TAB_META: Record<TabKind, TabMeta> = {
  home: { title: 'Home', icon: 'sparkles' },
  salon: { title: 'Conversation', icon: 'chat' },
  'salon-list': { title: 'Chats', icon: 'chat' },
  terminal: { title: 'Terminal', icon: 'code' },
  document: { title: 'Document', icon: 'file' },
  aurora: { title: 'Characters', icon: 'characters' },
  prospero: { title: 'Projects', icon: 'projects' },
  scriptorium: { title: 'The Scriptorium', icon: 'scriptorium' },
  settings: { title: 'The Foundry', icon: 'settings' },
  files: { title: 'Files', icon: 'files' },
  photos: { title: 'My Photos', icon: 'photos' },
  scenarios: { title: 'Scenarios', icon: 'scenarios' },
  brahma: { title: 'Brahma Console', icon: 'brahma-console' },
  wardrobe: { title: 'The Wardrobe', icon: 'wardrobe' },
  profile: { title: 'Profile', icon: 'profile' },
  about: { title: 'About', icon: 'info' },
  'generate-image': { title: 'Generate Image', icon: 'image' },
  'document-standalone': { title: 'Document', icon: 'file' },
  'character-new': { title: 'New Character', icon: 'user-plus' },
  'character-edit': { title: 'Edit Character', icon: 'pencil' },
  'character-view': { title: 'Character', icon: 'characters' },
  'settings-wizard': { title: 'Provider Setup', icon: 'wand' },
  'custom-tools': { title: "Pascal's Workbench", icon: 'wrench' },
  // v5-ONLY (P4.d16 tier 2) — v4 has no such kind (New Chat is its modal), so
  // this row is excluded from the corpus comparison below.
  'salon-new': { title: 'New Chat', icon: 'chat' },
};

export function defaultTabMeta(kind: TabKind): TabMeta {
  return DEFAULT_TAB_META[kind];
}
