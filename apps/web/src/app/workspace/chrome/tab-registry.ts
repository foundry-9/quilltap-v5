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
 *    (`TabPortalHost`); the Salon (lane J2's `SalonModePanes`) supplies the
 *    portal SOURCE.
 *  - **input-driven screen** — salon/settings/character-edit/character-view/
 *    custom-tools bind lane J2's §2 signal inputs from the tab payload
 *    (ACTIVATED at the p4.9j unification); wardrobe / document-standalone /
 *    brahma joined at the workspace-tabs remainder round's unification
 *    (P4.9J3 / P4.9J4 / P4.9I1B — all 22 kinds are now hosted; the loud
 *    `NotWiredPane` refusal scaffold retired with its last consumer).
 *
 * The map is behind an injection token so the keep-alive spec can substitute a
 * counting stub, and so lane screens can be swapped in at unification by
 * editing one table.
 *
 * @module workspace/chrome/tab-registry
 */

import { InjectionToken, type Type } from '@angular/core';

import type { TabKind, WorkspaceTab } from '../workspace-contract';
import { TabPortalHost } from './tab-portal-host';

// In-lane no-input screens.
import { HomePage } from '../../screens/home/home-page';
import { SalonList } from '../../screens/salon/salon-list';
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

// Input-driven screens (lane J2's §2 dual-mode inputs, bound at unification).
import { SalonConversation } from '../../screens/salon/salon-conversation';
import { Settings } from '../../screens/settings/settings';
import { CharacterEdit } from '../../screens/characters/edit/character-edit';
import { CharacterDetail } from '../../screens/characters/view/character-detail';
import { CustomToolsPage } from '../../screens/custom-tools/custom-tools-page';
import type {
  CharacterEditTabPayload,
  CharacterViewTabPayload,
  CustomToolsTabPayload,
  DocumentStandaloneTabPayload,
  SalonTabPayload,
  SettingsTabPayload,
  WardrobeTabPayload,
} from '../workspace-contract';

// The workspace-tabs remainder round's screens (bound at its unification).
import { WardrobeTabView } from '../../wardrobe/wardrobe-control-dialog';
import { StandaloneDocumentView } from '../../documents/standalone-document-view';
import { BrahmaConsoleView } from '../../brahma/brahma-console-view';

export interface TabViewEntry {
  component: Type<unknown>;
  /** Inputs to bind on the hosted component (re-applied on payload refresh). */
  inputs?: (tab: WorkspaceTab) => Record<string, unknown>;
}

export type TabRegistry = Record<TabKind, TabViewEntry>;

export const TAB_VIEW_REGISTRY = new InjectionToken<TabRegistry>('quilltap.workspace.tabRegistry');

export const DEFAULT_TAB_REGISTRY: TabRegistry = {
  // --- in-lane real screens (no inputs) ---
  home: { component: HomePage },
  // The all-chats list (v4 `8d86847a`: `salon-list` → `<SalonListView />`). Its
  // chat cards are plain `/salon/:id` links — the host's interceptor turns each
  // click into a Salon tab, so the list needs no inputs and no tab seams.
  'salon-list': { component: SalonList },
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

  // --- input-driven screens (lane J2's §2 inputs, activated at unification) ---
  salon: {
    component: SalonConversation,
    inputs: (tab) => ({ chatId: (tab.payload as SalonTabPayload | undefined)?.chatId ?? null }),
  },
  settings: {
    component: Settings,
    inputs: (tab) => {
      const p = tab.payload as SettingsTabPayload | undefined;
      return { tab: p?.tab ?? null, section: p?.section ?? null };
    },
  },
  'character-edit': {
    component: CharacterEdit,
    inputs: (tab) => {
      const p = tab.payload as CharacterEditTabPayload | undefined;
      return { characterId: p?.characterId ?? null, tab: p?.tab ?? null };
    },
  },
  'character-view': {
    component: CharacterDetail,
    inputs: (tab) => {
      const p = tab.payload as CharacterViewTabPayload | undefined;
      return { characterId: p?.characterId ?? null, tab: p?.tab ?? null };
    },
  },
  'custom-tools': {
    component: CustomToolsPage,
    inputs: (tab) => {
      const p = tab.payload as CustomToolsTabPayload | undefined;
      return { mountPointId: p?.mountPointId ?? null, path: p?.path ?? null, create: p?.create ?? false };
    },
  },

  // --- the workspace-tabs remainder round (bound at its unification) ---
  wardrobe: {
    component: WardrobeTabView,
    inputs: (tab) => ({
      characterId: (tab.payload as WardrobeTabPayload | undefined)?.characterId,
    }),
  },
  'document-standalone': {
    component: StandaloneDocumentView,
    inputs: (tab) => ({ payload: tab.payload as DocumentStandaloneTabPayload }),
  },
  brahma: { component: BrahmaConsoleView },
};
