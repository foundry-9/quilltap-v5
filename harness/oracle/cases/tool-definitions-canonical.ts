/**
 * Oracle case: canonicalized full tool catalog (wave 4 / W4.1b.3 spot-check).
 *
 * Drives v4's REAL `canonicalizeUniversalTools` over the ENTIRE tool catalog
 * (every `*ToolDefinition` as a `UniversalTool`) and emits the canonicalized
 * array as a single `JSON.stringify` line. Proves the Rust
 * `canonicalize_universal_tools` (ICU name sort + deep key sort) over real data,
 * whose own differential predated having real definitions.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/tool-definitions-canonical.ts \
 *     > /tmp/oracle-tool-definitions-canonical.ndjson
 */

import { canonicalizeUniversalTools } from '@/lib/tools/canonicalize';
import type { UniversalTool } from '@/lib/plugins/interfaces/tool-plugin';

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
import { memorySearchToolDefinition } from '@/lib/tools/memory-search-tool';
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

const ALL: UniversalTool[] = [
  askCarinaToolDefinition,
  attachImageToolDefinition,
  deleteAnnotationToolDefinition,
  docCloseDocumentToolDefinition,
  docCopyFileToolDefinition,
  docCreateFolderToolDefinition,
  docDeleteBlobToolDefinition,
  docDeleteFileToolDefinition,
  docDeleteFolderToolDefinition,
  docFocusToolDefinition,
  docGrepToolDefinition,
  docInsertTextToolDefinition,
  docListBlobsToolDefinition,
  docListFilesToolDefinition,
  docMoveFileToolDefinition,
  docMoveFolderToolDefinition,
  docOpenDocumentToolDefinition,
  docReadBlobToolDefinition,
  docReadFileToolDefinition,
  docReadFrontmatterToolDefinition,
  docReadHeadingToolDefinition,
  docStrReplaceToolDefinition,
  docUpdateFrontmatterToolDefinition,
  docUpdateHeadingToolDefinition,
  docWriteBlobToolDefinition,
  docWriteFileToolDefinition,
  helpNavigateToolDefinition,
  helpSearchToolDefinition,
  helpSettingsToolDefinition,
  imageGenerationToolDefinition,
  keepImageToolDefinition,
  listEmailToolDefinition,
  listImagesToolDefinition,
  memorySearchToolDefinition,
  sendMailToolDefinition,
  projectInfoToolDefinition,
  readConversationToolDefinition,
  requestFullContextToolDefinition,
  rngToolDefinition,
  runCustomToolDefinition,
  runSqlToolDefinition,
  searchScriptoriumToolDefinition,
  searchScriptoriumBrahmaToolDefinition,
  selfInventoryToolDefinition,
  stateToolDefinition,
  submitFinalResponseToolDefinition,
  terminalListToolDefinition,
  terminalReadToolDefinition,
  upsertAnnotationToolDefinition,
  wardrobeListToolDefinition,
  wardrobeReadToolDefinition,
  wardrobeCreateToolDefinition,
  wardrobeUpdateToolDefinition,
  wardrobeArchiveToolDefinition,
  wardrobeWearToolDefinition,
  wardrobeTakeOffToolDefinition,
  webSearchToolDefinition,
  whisperToolDefinition,
] as unknown as UniversalTool[];

const canonical = canonicalizeUniversalTools(ALL);
process.stdout.write(JSON.stringify({ canonicalized: canonical }) + '\n');
