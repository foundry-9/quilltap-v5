import { computed, inject, Injectable, signal } from '@angular/core';

import { CoreClient } from '../../core/core-client';
import { notifyQueueChange } from '../../layout/queue-status.logic';
import * as api from './scriptorium.api';
import type {
  CreateDocumentStoreData,
  DocumentStore,
  UpdateDocumentStoreData,
} from './scriptorium.api';

/** A transient success/error notice (the SPA stand-in for v4's toasts). */
export interface Flash {
  kind: 'success' | 'error';
  message: string;
}

/**
 * The Scriptorium list store (v4 `useDocumentStores`): the `stores` array +
 * loading/error, with the v4 patch-not-refetch shape — create PREPENDS,
 * update REPLACES, delete FILTERS, and scan/convert/deconvert re-GET the ONE
 * store and splice it back (a full-list refetch would be a behavior change).
 * Success/failure surface through {@link flash} (v4's toast calls). Provided at
 * the list component so its state is per-screen.
 */
@Injectable()
export class ScriptoriumStore {
  private readonly core = inject(CoreClient);

  private readonly _stores = signal<DocumentStore[]>([]);
  private readonly _loading = signal(true);
  private readonly _error = signal<string | null>(null);
  private readonly _flash = signal<Flash | null>(null);

  readonly stores = this._stores.asReadonly();
  readonly loading = this._loading.asReadonly();
  readonly error = this._error.asReadonly();
  readonly flash = this._flash.asReadonly();

  readonly isEmpty = computed(() => this._stores().length === 0);

  clearFlash(): void {
    this._flash.set(null);
  }

  async fetchStores(): Promise<void> {
    this._loading.set(true);
    this._error.set(null);
    try {
      this._stores.set(await api.fetchStores(this.core));
    } catch (err) {
      this._error.set(message(err, 'An error occurred'));
    } finally {
      this._loading.set(false);
    }
  }

  async createStore(data: CreateDocumentStoreData): Promise<DocumentStore | null> {
    try {
      const result = await api.createStore(this.core, data);
      this._stores.update((prev) => [result.mountPoint, ...prev]);
      if (result.warning) {
        this._flash.set({ kind: 'error', message: result.warning });
      } else {
        this._flash.set({ kind: 'success', message: 'Document store created successfully!' });
      }
      return result.mountPoint;
    } catch (err) {
      this._flash.set({ kind: 'error', message: message(err, 'Failed to create document store') });
      return null;
    }
  }

  async updateStore(
    id: string,
    data: UpdateDocumentStoreData,
  ): Promise<DocumentStore | null> {
    try {
      const mountPoint = await api.updateStore(this.core, id, data);
      this._stores.update((prev) => prev.map((s) => (s.id === id ? mountPoint : s)));
      this._flash.set({ kind: 'success', message: 'Document store updated successfully!' });
      return mountPoint;
    } catch (err) {
      this._flash.set({ kind: 'error', message: message(err, 'Failed to update document store') });
      return null;
    }
  }

  async deleteStore(id: string): Promise<boolean> {
    try {
      await api.deleteStore(this.core, id);
      this._stores.update((prev) => prev.filter((s) => s.id !== id));
      this._flash.set({ kind: 'success', message: 'Document store deleted successfully!' });
      return true;
    } catch (err) {
      this._flash.set({ kind: 'error', message: message(err, 'Failed to delete document store') });
      return false;
    }
  }

  async scanStore(id: string): Promise<boolean> {
    this.patch(id, { scanStatus: 'scanning' });
    try {
      const result = await api.scanStore(this.core, id);
      await this.regetAndSplice(id);
      this._flash.set({
        kind: 'success',
        message: `Scan complete: ${result.scanResult.filesScanned} files scanned, ${result.embeddingJobsEnqueued} embedding jobs queued`,
      });
      // v4 wakes the queue badges when a scan enqueued embedding jobs
      // (useDocumentStores:141 / useDocumentStoreDetail:126 — v5's one shared
      // store covers both call sites), guarded on the count exactly as v4 is.
      if (result.embeddingJobsEnqueued > 0) {
        notifyQueueChange();
      }
      return true;
    } catch (err) {
      this._flash.set({ kind: 'error', message: message(err, 'Failed to scan document store') });
      this.patch(id, { scanStatus: 'error' });
      return false;
    }
  }

  async convertStore(id: string): Promise<boolean> {
    this.patch(id, { conversionStatus: 'converting', conversionError: null });
    try {
      await api.convertStore(this.core, id);
      await this.regetAndSplice(id);
      this._flash.set({ kind: 'success', message: 'Converted to database.' });
      return true;
    } catch (err) {
      const msg = message(err, 'Failed to convert document store');
      this._flash.set({ kind: 'error', message: msg });
      this.patch(id, { conversionStatus: 'error', conversionError: msg });
      return false;
    }
  }

  async deconvertStore(id: string, targetPath: string): Promise<boolean> {
    this.patch(id, { conversionStatus: 'deconverting', conversionError: null });
    try {
      await api.deconvertStore(this.core, id, targetPath);
      await this.regetAndSplice(id);
      this._flash.set({
        kind: 'success',
        message: `Deconverted to filesystem: written to ${targetPath}`,
      });
      return true;
    } catch (err) {
      const msg = message(err, 'Failed to deconvert document store');
      this._flash.set({ kind: 'error', message: msg });
      this.patch(id, { conversionStatus: 'error', conversionError: msg });
      return false;
    }
  }

  /** Re-GET the one store and splice it into the list (v4 patch-not-refetch). */
  private async regetAndSplice(id: string): Promise<void> {
    try {
      const fresh = await api.fetchStore(this.core, id);
      this._stores.update((prev) => prev.map((s) => (s.id === id ? fresh : s)));
    } catch {
      /* v4 only splices if the re-GET is ok; otherwise the optimistic row stays. */
    }
  }

  /** Optimistic single-store field patch. */
  private patch(id: string, fields: Partial<DocumentStore>): void {
    this._stores.update((prev) => prev.map((s) => (s.id === id ? { ...s, ...fields } : s)));
  }
}

function message(err: unknown, fallback: string): string {
  return err instanceof Error ? err.message : fallback;
}
