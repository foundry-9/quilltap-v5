/**
 * Oracle case: the full tool-definition catalog (wave 4 / W4.1b.3).
 *
 * Imports EVERY `lib/tools/*-tool.ts` definition (cross-checked against the
 * snapshot test's `ALL_TOOLS`) and emits, per tool, one NDJSON line:
 *   { key, name, defJson }
 * where `defJson` is the byte-exact `JSON.stringify({ name, description,
 * parameters })` — the value computed at import time from the static Zod schema
 * via Zod 4's `z.toJSONSchema` (see `zod-to-openai-schema.ts`). The parameters
 * are static data, so the faithful port is these bytes, not the emitter.
 *
 * This file is BOTH the differential oracle and the input to the generator
 * (`gen-tool-catalog.ts`), so the stored Rust constants and the diff target can
 * never disagree.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/tool-definitions.ts \
 *     > /tmp/oracle-tool-definitions.ndjson
 */

import { askCarinaToolDefinition } from '@/lib/tools/ask-carina-tool';
import { attachImageToolDefinition } from '@/lib/tools/attach-image-tool';
import { deleteAnnotationToolDefinition } from '@/lib/tools/delete-annotation-tool';
import { docCloseDocumentToolDefinition } from '@/lib/tools/doc-close-document-tool';
import { docCopyFileToolDefinition } from '@/lib/tools/doc-copy-file-tool';
import { docCreateFolderToolDefinition } from '@/lib/tools/doc-create-folder-tool';
import { docDeleteBlobToolDefinition } from '@/lib/tools/doc-delete-blob-tool';
import { docDeleteFileToolDefinition } from '@/lib/tools/doc-delete-file-tool';
import { docDeleteFolderToolDefinition } from '@/lib/tools/doc-delete-folder-tool';
import { docFocusToolDefinition } from '@/lib/tools/doc-focus-tool';
import { docGrepToolDefinition } from '@/lib/tools/doc-grep-tool';
import { docInsertTextToolDefinition } from '@/lib/tools/doc-insert-text-tool';
import { docListBlobsToolDefinition } from '@/lib/tools/doc-list-blobs-tool';
import { docListFilesToolDefinition } from '@/lib/tools/doc-list-files-tool';
import { docMoveFileToolDefinition } from '@/lib/tools/doc-move-file-tool';
import { docMoveFolderToolDefinition } from '@/lib/tools/doc-move-folder-tool';
import { docOpenDocumentToolDefinition } from '@/lib/tools/doc-open-document-tool';
import { docReadBlobToolDefinition } from '@/lib/tools/doc-read-blob-tool';
import { docReadFileToolDefinition } from '@/lib/tools/doc-read-file-tool';
import { docReadFrontmatterToolDefinition } from '@/lib/tools/doc-read-frontmatter-tool';
import { docReadHeadingToolDefinition } from '@/lib/tools/doc-read-heading-tool';
import { docStrReplaceToolDefinition } from '@/lib/tools/doc-str-replace-tool';
import { docUpdateFrontmatterToolDefinition } from '@/lib/tools/doc-update-frontmatter-tool';
import { docUpdateHeadingToolDefinition } from '@/lib/tools/doc-update-heading-tool';
import { docWriteBlobToolDefinition } from '@/lib/tools/doc-write-blob-tool';
import { docWriteFileToolDefinition } from '@/lib/tools/doc-write-file-tool';
import { helpNavigateToolDefinition } from '@/lib/tools/help-navigate-tool';
import { helpSearchToolDefinition } from '@/lib/tools/help-search-tool';
import { helpSettingsToolDefinition } from '@/lib/tools/help-settings-tool';
import { imageGenerationToolDefinition } from '@/lib/tools/image-generation-tool';
import { keepImageToolDefinition } from '@/lib/tools/keep-image-tool';
import { listEmailToolDefinition } from '@/lib/tools/list-email-tool';
import { listImagesToolDefinition } from '@/lib/tools/list-images-tool';
import { sendMailToolDefinition } from '@/lib/tools/send-mail-tool';
import { projectInfoToolDefinition } from '@/lib/tools/project-info-tool';
import { readConversationToolDefinition } from '@/lib/tools/read-conversation-tool';
import { requestFullContextToolDefinition } from '@/lib/tools/request-full-context-tool';
import { rngToolDefinition } from '@/lib/tools/rng-tool';
import { runCustomToolDefinition } from '@/lib/tools/run-custom-tool';
import { runSqlToolDefinition } from '@/lib/tools/run-sql-tool';
import {
  searchScriptoriumToolDefinition,
  searchScriptoriumBrahmaToolDefinition,
} from '@/lib/tools/search-scriptorium-tool';
import { selfInventoryToolDefinition } from '@/lib/tools/self-inventory-tool';
import { stateToolDefinition } from '@/lib/tools/state-tool';
import { submitFinalResponseToolDefinition } from '@/lib/tools/submit-final-response-tool';
import { terminalListToolDefinition } from '@/lib/tools/terminal-list-tool';
import { terminalReadToolDefinition } from '@/lib/tools/terminal-read-tool';
import { upsertAnnotationToolDefinition } from '@/lib/tools/upsert-annotation-tool';
import { wardrobeListToolDefinition } from '@/lib/tools/wardrobe-list-tool';
import { wardrobeReadToolDefinition } from '@/lib/tools/wardrobe-read-tool';
import { wardrobeCreateToolDefinition } from '@/lib/tools/wardrobe-create-tool';
import { wardrobeUpdateToolDefinition } from '@/lib/tools/wardrobe-update-tool';
import { wardrobeArchiveToolDefinition } from '@/lib/tools/wardrobe-archive-tool';
import { wardrobeWearToolDefinition } from '@/lib/tools/wardrobe-wear-tool';
import { wardrobeTakeOffToolDefinition } from '@/lib/tools/wardrobe-take-off-tool';
import { webSearchToolDefinition } from '@/lib/tools/web-search-tool';
import { whisperToolDefinition } from '@/lib/tools/whisper-tool';

const ALL_TOOLS: Record<string, { type: string; function: { name: string; description?: string; parameters?: unknown } }> = {
  askCarina: askCarinaToolDefinition,
  attachImage: attachImageToolDefinition,
  deleteAnnotation: deleteAnnotationToolDefinition,
  docCloseDocument: docCloseDocumentToolDefinition,
  docCopyFile: docCopyFileToolDefinition,
  docCreateFolder: docCreateFolderToolDefinition,
  docDeleteBlob: docDeleteBlobToolDefinition,
  docDeleteFile: docDeleteFileToolDefinition,
  docDeleteFolder: docDeleteFolderToolDefinition,
  docFocus: docFocusToolDefinition,
  docGrep: docGrepToolDefinition,
  docInsertText: docInsertTextToolDefinition,
  docListBlobs: docListBlobsToolDefinition,
  docListFiles: docListFilesToolDefinition,
  docMoveFile: docMoveFileToolDefinition,
  docMoveFolder: docMoveFolderToolDefinition,
  docOpenDocument: docOpenDocumentToolDefinition,
  docReadBlob: docReadBlobToolDefinition,
  docReadFile: docReadFileToolDefinition,
  docReadFrontmatter: docReadFrontmatterToolDefinition,
  docReadHeading: docReadHeadingToolDefinition,
  docStrReplace: docStrReplaceToolDefinition,
  docUpdateFrontmatter: docUpdateFrontmatterToolDefinition,
  docUpdateHeading: docUpdateHeadingToolDefinition,
  docWriteBlob: docWriteBlobToolDefinition,
  docWriteFile: docWriteFileToolDefinition,
  helpNavigate: helpNavigateToolDefinition,
  helpSearch: helpSearchToolDefinition,
  helpSettings: helpSettingsToolDefinition,
  imageGeneration: imageGenerationToolDefinition,
  keepImage: keepImageToolDefinition,
  listEmail: listEmailToolDefinition,
  listImages: listImagesToolDefinition,
  sendMail: sendMailToolDefinition,
  projectInfo: projectInfoToolDefinition,
  readConversation: readConversationToolDefinition,
  requestFullContext: requestFullContextToolDefinition,
  rng: rngToolDefinition,
  runCustom: runCustomToolDefinition,
  runSql: runSqlToolDefinition,
  searchScriptorium: searchScriptoriumToolDefinition,
  searchScriptoriumBrahma: searchScriptoriumBrahmaToolDefinition,
  selfInventory: selfInventoryToolDefinition,
  state: stateToolDefinition,
  submitFinalResponse: submitFinalResponseToolDefinition,
  terminalList: terminalListToolDefinition,
  terminalRead: terminalReadToolDefinition,
  upsertAnnotation: upsertAnnotationToolDefinition,
  wardrobeList: wardrobeListToolDefinition,
  wardrobeRead: wardrobeReadToolDefinition,
  wardrobeCreate: wardrobeCreateToolDefinition,
  wardrobeUpdate: wardrobeUpdateToolDefinition,
  wardrobeArchive: wardrobeArchiveToolDefinition,
  wardrobeWear: wardrobeWearToolDefinition,
  wardrobeTakeOff: wardrobeTakeOffToolDefinition,
  webSearch: webSearchToolDefinition,
  whisper: whisperToolDefinition,
};

for (const [key, tool] of Object.entries(ALL_TOOLS)) {
  const def = {
    name: tool.function.name,
    description: tool.function.description,
    parameters: tool.function.parameters,
  };
  const defJson = JSON.stringify(def);
  process.stdout.write(JSON.stringify({ key, name: tool.function.name, defJson }) + '\n');
}
