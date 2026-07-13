/**
 * The chat-file multipart upload leg (v4 `useFileAttachments.uploadFile`). The
 * byte body can't ride the JSON dispatch, so it POSTs `multipart/form-data` to
 * the web edge route `POST /api/v1/chats/{id}/files` (the P4.6m multipart
 * precedent). The response is one of three shapes: `{file}` on success,
 * `{duplicate, …}` on a name/content conflict, or `{error}`.
 */

/** A conflict resolution (v4 `ConflictResolution`). */
export type ConflictResolution = 'replace' | 'keepBoth' | 'skip';

/** An uploaded chat file (v4 `AttachedFile`). */
export interface UploadedChatFile {
  id: string;
  filename: string;
  filepath: string;
  mimeType: string;
  url: string;
}

/** The duplicate-conflict payload (v4 `FileConflictInfo` + `UploadDuplicateResponse`). */
export interface FileConflictInfo {
  conflictType: 'filename' | 'content' | 'both';
  existingFile: { id: string; filename: string; size: number; createdAt: string; sha256: string };
  newFile: { filename: string; size: number; sha256: string };
}

export type ChatFileUploadResult =
  | { kind: 'file'; file: UploadedChatFile }
  | { kind: 'duplicate'; conflict: FileConflictInfo };

/**
 * Upload one file to a chat, optionally with a conflict resolution. Returns the
 * uploaded file, or a duplicate conflict for the caller to resolve; throws on an
 * `{error}` body or a non-OK status (v4's error path).
 */
export async function uploadChatFile(
  chatId: string,
  file: File,
  opts?: { resolution?: ConflictResolution; conflictingFileId?: string },
): Promise<ChatFileUploadResult> {
  const form = new FormData();
  form.append('file', file);
  if (opts?.resolution) {
    form.append('resolution', opts.resolution);
  }
  if (opts?.conflictingFileId) {
    form.append('conflictingFileId', opts.conflictingFileId);
  }

  const res = await fetch(`/api/v1/chats/${encodeURIComponent(chatId)}/files`, {
    method: 'POST',
    body: form,
  });

  const data = (await res.json().catch(() => ({}))) as {
    file?: UploadedChatFile;
    duplicate?: boolean;
    conflictType?: FileConflictInfo['conflictType'];
    existingFile?: FileConflictInfo['existingFile'];
    newFile?: FileConflictInfo['newFile'];
    error?: string;
  };

  if (data.duplicate && data.conflictType && data.existingFile && data.newFile) {
    return {
      kind: 'duplicate',
      conflict: {
        conflictType: data.conflictType,
        existingFile: data.existingFile,
        newFile: data.newFile,
      },
    };
  }
  if (data.error) {
    throw new Error(data.error);
  }
  if (!res.ok || !data.file) {
    throw new Error(`Failed to upload file (HTTP ${res.status})`);
  }
  return { kind: 'file', file: data.file };
}
