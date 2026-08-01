import { computed, inject, Injectable, signal } from '@angular/core';

import { CoreClient } from '../../core/core-client';
import { ToastService } from '../../ui/toast.service';
import { notifyQueueChange } from '../../layout/queue-status.logic';
import * as api from './scriptorium.api';
import type {
  CreateDocumentStoreData,
  DocumentStore,
  UpdateDocumentStoreData,
} from './scriptorium.api';

/**
 * The Scriptorium list store (v4 `useDocumentStores`): the `stores` array +
 * loading/error, with the v4 patch-not-refetch shape — create PREPENDS,
 * update REPLACES, delete FILTERS, and scan/convert/deconvert re-GET the ONE
 * store and splice it back (a full-list refetch would be a behavior change).
 * Success/failure surface as toasts, exactly where v4 raises them. Provided at
 * the list component so its state is per-screen.
 */
@Injectable()
export class ScriptoriumStore {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  private readonly _stores = signal<DocumentStore[]>([]);
  private readonly _loading = signal(true);
  private readonly _error = signal<string | null>(null);

  readonly stores = this._stores.asReadonly();
  readonly loading = this._loading.asReadonly();
  readonly error = this._error.asReadonly();

  readonly isEmpty = computed(() => this._stores().length === 0);

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
        this.toasts.showError(result.warning);
      } else {
        this.toasts.showSuccess('Document store created successfully!');
      }
      return result.mountPoint;
    } catch (err) {
      this.toasts.showError(message(err, 'Failed to create document store'));
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
      this.toasts.showSuccess('Document store updated successfully!');
      return mountPoint;
    } catch (err) {
      this.toasts.showError(message(err, 'Failed to update document store'));
      return null;
    }
  }

  async deleteStore(id: string): Promise<boolean> {
    try {
      await api.deleteStore(this.core, id);
      this._stores.update((prev) => prev.filter((s) => s.id !== id));
      this.toasts.showSuccess('Document store deleted successfully!');
      return true;
    } catch (err) {
      this.toasts.showError(message(err, 'Failed to delete document store'));
      return false;
    }
  }

  async scanStore(id: string): Promise<boolean> {
    this.patch(id, { scanStatus: 'scanning' });
    try {
      const result = await api.scanStore(this.core, id);
      await this.regetAndSplice(id);
      this.toasts.showSuccess(
        `Scan complete: ${result.scanResult.filesScanned} files scanned, ${result.embeddingJobsEnqueued} embedding jobs queued`,
      );
      // v4 wakes the queue badges when a scan enqueued embedding jobs
      // (useDocumentStores:141 / useDocumentStoreDetail:126 — v5's one shared
      // store covers both call sites), guarded on the count exactly as v4 is.
      if (result.embeddingJobsEnqueued > 0) {
        notifyQueueChange();
      }
      return true;
    } catch (err) {
      this.toasts.showError(message(err, 'Failed to scan document store'));
      this.patch(id, { scanStatus: 'error' });
      return false;
    }
  }

  async convertStore(id: string): Promise<boolean> {
    this.patch(id, { conversionStatus: 'converting', conversionError: null });
    try {
      await api.convertStore(this.core, id);
      await this.regetAndSplice(id);
      this.toasts.showSuccess('Converted to database.');
      return true;
    } catch (err) {
      const msg = message(err, 'Failed to convert document store');
      this.toasts.showError(msg);
      this.patch(id, { conversionStatus: 'error', conversionError: msg });
      return false;
    }
  }

  async deconvertStore(id: string, targetPath: string): Promise<boolean> {
    this.patch(id, { conversionStatus: 'deconverting', conversionError: null });
    try {
      await api.deconvertStore(this.core, id, targetPath);
      await this.regetAndSplice(id);
      this.toasts.showSuccess(
        `Deconverted to filesystem: written to ${targetPath}`,
      );
      return true;
    } catch (err) {
      const msg = message(err, 'Failed to deconvert document store');
      this.toasts.showError(msg);
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
