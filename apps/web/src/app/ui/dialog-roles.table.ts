/**
 * The dialog-roles table (P4.D358 item 7) — one row per modal overlay in the
 * SPA, mirroring v4 `dff76ebef`'s per-file dialog pass (+ `58120a7fe`'s
 * `BaseModal`). `ui/dialog-roles.spec.ts` reads each `v5` file's SOURCE and
 * asserts the template carries an element with `role="<role>"`,
 * `aria-modal="true"` (`"false"` for a `nonModal` row) and a name binding of the given kind — so a later edit
 * that drops a dialog's semantics fails `npm test`.
 *
 * `name`: `labelledby` = `aria-labelledby` (a title element with the id),
 * `label` = `aria-label` (v4 names a title-less image viewer / the search
 * panel that way).
 *
 * `owner`: a SIBLING lane owns the file this round and applies the row as a
 * HANDOFF; the spec skips it with the owner's name until the unifier removes
 * the field (§S.4 — the same emptying as `scripts/a11y-names-allowlist.json`).
 *
 * Every `qt-modal` host inherits the `ui/modal.ts` row (76 hosts at P4.D358).
 */
export interface DialogRoleRow {
  /** Path under `apps/web/src/app`. */
  readonly v5: string;
  /** v4's twin at `236c04e35` (`—` where v5's surface has no v4 twin). */
  readonly v4: string;
  readonly role: 'dialog' | 'alertdialog';
  readonly name: 'labelledby' | 'label';
  /** C9 R2's NON-modal floating surface: `aria-modal="false"` (default: modal). */
  readonly nonModal?: true;
  readonly owner?: 'P4.D359' | 'P4.D346';
}

export const DIALOG_ROLES: ReadonlyArray<DialogRoleRow> = [
  { v5: 'ui/modal.ts', v4: 'components/ui/BaseModal.tsx', role: 'dialog', name: 'labelledby' },
  { v5: 'ui/alert-modal.service.ts', v4: 'components/alert-dialog.tsx', role: 'alertdialog', name: 'labelledby' },
  { v5: 'chat/memory-cascade-dialog.ts', v4: 'components/ui/MemoryCascadeDialog.tsx', role: 'alertdialog', name: 'labelledby' },
  { v5: 'images/image-detail-modal.ts', v4: 'components/images/image-detail/ImageDetailModal.tsx', role: 'dialog', name: 'label' },
  { v5: 'images/photo-gallery-modal.ts', v4: 'components/images/PhotoGalleryModal.tsx', role: 'dialog', name: 'labelledby' },
  { v5: 'images/image-modal.ts', v4: '—', role: 'dialog', name: 'label' },
  { v5: 'images/full-screen-image-viewer.ts', v4: 'components/images/FullScreenImageViewer.tsx', role: 'dialog', name: 'label' },
  { v5: 'progressions/progression-editor-modal.ts', v4: 'components/characters/progressions/ProgressionEditorModal.tsx', role: 'dialog', name: 'labelledby' },
  { v5: 'screens/characters/generators/ai-import/ai-import-wizard.ts', v4: 'components/settings/ai-import/AIImportWizard.tsx', role: 'dialog', name: 'labelledby' },
  { v5: 'screens/characters/generators/external-prompt/external-prompt-dialog.ts', v4: 'app/aurora/[id]/view/components/ExternalPromptDialog.tsx', role: 'dialog', name: 'labelledby' },
  { v5: 'screens/characters/generators/external-prompt/external-prompt-result-dialog.ts', v4: 'app/aurora/[id]/view/components/ExternalPromptResultDialog.tsx', role: 'dialog', name: 'labelledby' },
  { v5: 'screens/characters/generators/optimizer/character-optimizer-modal.ts', v4: 'components/characters/optimizer/CharacterOptimizerModal.tsx', role: 'dialog', name: 'labelledby' },
  { v5: 'screens/characters/generators/wizard/ai-wizard-modal.ts', v4: 'components/characters/ai-wizard/AIWizardModal.tsx', role: 'dialog', name: 'labelledby' },
  { v5: 'search/search-dialog.ts', v4: 'components/search/search-dialog.tsx', role: 'dialog', name: 'label' },
  { v5: 'wardrobe/item-editor/wardrobe-mode-change-prompt.ts', v4: 'components/wardrobe/wardrobe-item-editor/WardrobeModeChangePrompt.tsx', role: 'alertdialog', name: 'labelledby' },
  { v5: 'screens/custom-tools/destination-picker.ts', v4: 'components/custom-tools/DestinationPicker.tsx', role: 'dialog', name: 'labelledby' },
  { v5: 'screens/custom-tools/workbench-editor.ts', v4: 'components/custom-tools/WorkbenchEditor.tsx', role: 'alertdialog', name: 'labelledby' },
  { v5: 'help/help-dialog.ts', v4: 'components/help-chat/HelpChatDialog.tsx', role: 'dialog', name: 'labelledby' },
  { v5: 'chat/bulk-character-replace-modal.ts', v4: 'components/chat/BulkCharacterReplaceModal.tsx', role: 'dialog', name: 'labelledby' },
  { v5: 'chat/cast/create-npc-dialog.ts', v4: 'components/chat/CreateNPCDialog.tsx', role: 'dialog', name: 'labelledby' },
  { v5: 'chat/merge-conversation-modal.ts', v4: 'components/chat/MergeConversationModal.tsx', role: 'dialog', name: 'labelledby' },
  { v5: 'chat/post-office/whisper-dialog.ts', v4: 'components/chat/WhisperDialog.tsx', role: 'dialog', name: 'labelledby' },
  { v5: 'chat/reattribute-message-dialog.ts', v4: 'components/chat/ReattributeMessageDialog.tsx', role: 'dialog', name: 'labelledby' },
  { v5: 'chat/cast/add-character-dialog.ts', v4: 'components/chat/AddCharacterDialog.tsx', role: 'dialog', name: 'labelledby' },
  { v5: 'memory/maintenance-dialog.ts', v4: 'components/memory/maintenance-dialog.tsx', role: 'dialog', name: 'labelledby' },
  { v5: 'memory/memory-digest-viewer.ts', v4: 'components/memory/memory-list.tsx', role: 'dialog', name: 'labelledby' },
  { v5: 'screens/new-chat/green-room-dialog.ts', v4: 'components/new-chat/ChatCreationProgressModal.tsx', role: 'dialog', name: 'labelledby' },
  { v5: 'screens/profile/avatar-picker.ts', v4: 'components/images/avatar-selector.tsx', role: 'dialog', name: 'labelledby' },
  { v5: 'almanack/almanack-report-dialog.ts', v4: 'components/tools/capabilities-report-dialog.tsx', role: 'dialog', name: 'labelledby' },
  { v5: 'screens/files/file-preview-modal.ts', v4: 'components/files/FilePreview/FilePreviewModal.tsx', role: 'dialog', name: 'labelledby' },
  { v5: 'screens/photos/photos-page.ts', v4: 'app/photos/PhotosView.tsx', role: 'dialog', name: 'labelledby' },
  // Sibling-owned this round (HANDOFFs H-out-4 / -5 / -6): skipped until applied.
  { v5: 'memory/memory-list.ts', v4: 'components/memory/memory-list.tsx', role: 'dialog', name: 'labelledby', owner: 'P4.D359' },
  { v5: 'wardrobe/wardrobe-item-editor.ts', v4: 'components/wardrobe/wardrobe-item-editor/WardrobeItemEditor.tsx', role: 'dialog', name: 'labelledby', owner: 'P4.D359' },
  { v5: 'brahma/brahma-console-dialog.ts', v4: 'components/ui/FloatingDialog.tsx', role: 'dialog', name: 'labelledby', nonModal: true, owner: 'P4.D359' },
];
