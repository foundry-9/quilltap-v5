/**
 * The kind → hosted-component map (v4 `TabView.tsx` render switch, baseline
 * `b8b12695`) plus the v5 in-lane hosting decisions (P4.9J1 deliverable 5).
 *
 * A tab kind resolves to a component and an optional inputs factory. Three kinds
 * of entry:
 *  - **in-lane real screen** — the v5 screen that needs no inputs today
 *    (home/aurora/prospero/scriptorium/files/photos/scenarios/generate-image/
 *    about/profile/character-new/settings-wizard).
 *  - **portal host** — terminal/document render the J1 portal-host chrome
 *    (`TabPortalHost`); lane J2 supplies the salon-side portal SOURCE at unify.
 *  - **loud not-yet-wired pane** — salon/settings/wardrobe/character-edit/
 *    character-view/custom-tools/document-standalone render `NotWiredPane`
 *    (ACTIVATE-AT-UNIFY over lane J2's §2 inputs); `brahma` renders a permanent
 *    refusal pane naming `p4.9i1`.
 *
 * The map is behind an injection token so the keep-alive spec can substitute a
 * counting stub, and so lane J2's screens can be swapped in at unification by
 * editing one table.
 *
 * @module workspace/chrome/tab-registry
 */

import { InjectionToken, type Type } from '@angular/core';

import type { TabKind, WorkspaceTab } from '../workspace-contract';
import { NotWiredPane } from './not-wired-pane';
import { TabPortalHost } from './tab-portal-host';

// In-lane no-input screens.
import { HomePage } from '../../screens/home/home-page';
import { CharactersList } from '../../screens/characters/list/characters-list';
import { ProsperoList } from '../../screens/prospero/prospero-list';
import { ScriptoriumList } from '../../screens/scriptorium/scriptorium-list';
import { FilesBrowser } from '../../screens/files/files-browser';
import { PhotosPage } from '../../screens/photos/photos-page';
import { ScenariosPage } from '../../screens/scenarios/scenarios-page';
import { GenerateImagePage } from '../../screens/generate-image/generate-image-page';
import { AboutPage } from '../../screens/about/about-page';
import { ProfilePage } from '../../screens/profile/profile-page';
import { NewCharacter } from '../../screens/characters/new/new-character';
import { WizardScreen } from '../../screens/settings/wizard/wizard-screen';

export interface TabViewEntry {
  component: Type<unknown>;
  /** Inputs to bind on the hosted component (re-applied on payload refresh). */
  inputs?: (tab: WorkspaceTab) => Record<string, unknown>;
}

export type TabRegistry = Record<TabKind, TabViewEntry>;

export const TAB_VIEW_REGISTRY = new InjectionToken<TabRegistry>('quilltap.workspace.tabRegistry');

/** ACTIVATE-AT-UNIFY / permanent-refusal entry: render the loud pane. */
function refusal(kind: TabKind): TabViewEntry {
  return { component: NotWiredPane, inputs: () => ({ kind }) };
}

export const DEFAULT_TAB_REGISTRY: TabRegistry = {
  // --- in-lane real screens (no inputs) ---
  home: { component: HomePage },
  aurora: { component: CharactersList },
  prospero: { component: ProsperoList },
  scriptorium: { component: ScriptoriumList },
  files: { component: FilesBrowser },
  photos: { component: PhotosPage },
  scenarios: { component: ScenariosPage },
  'generate-image': { component: GenerateImagePage },
  about: { component: AboutPage },
  profile: { component: ProfilePage },
  'character-new': { component: NewCharacter },
  'settings-wizard': { component: WizardScreen },

  // --- portal hosts (J1 chrome; J2 supplies the source at unify) ---
  terminal: {
    component: TabPortalHost,
    inputs: (tab) => ({
      kind: 'terminal',
      chatId: (tab.payload as { chatId?: string } | undefined)?.chatId ?? '',
    }),
  },
  document: {
    component: TabPortalHost,
    inputs: (tab) => {
      const p = tab.payload as { chatId?: string; chatDocumentId?: string } | undefined;
      return { kind: 'document', chatId: p?.chatId ?? '', docId: p?.chatDocumentId };
    },
  },

  // --- loud not-yet-wired panes (ACTIVATE-AT-UNIFY) + permanent brahma refusal ---
  salon: refusal('salon'),
  settings: refusal('settings'),
  wardrobe: refusal('wardrobe'),
  'character-edit': refusal('character-edit'),
  'character-view': refusal('character-view'),
  'custom-tools': refusal('custom-tools'),
  'document-standalone': refusal('document-standalone'),
  brahma: refusal('brahma'),
};
