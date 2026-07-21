/**
 * BrahmaToolCall — asserted against v4 `BrahmaToolCall.tsx`: the header carries
 * the target database + a status chip (row count / Failed / Running…), a
 * successful query renders its rows as a table (NULLs dimmed), a failure shows
 * the error text, an empty result says so, and a pending call shows the working
 * indicator.
 */

import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import type { BrahmaSqlToolCallData } from './brahma-sql-tool-call';
import { BrahmaToolCall } from './brahma-tool-call';

async function render(data: BrahmaSqlToolCallData): Promise<ComponentFixture<BrahmaToolCall>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [BrahmaToolCall] });
  const fixture = TestBed.createComponent(BrahmaToolCall);
  fixture.componentRef.setInput('data', data);
  fixture.detectChanges();
  return fixture;
}

function text(fixture: ComponentFixture<BrahmaToolCall>): string {
  return (fixture.nativeElement as HTMLElement).textContent ?? '';
}

describe('BrahmaToolCall (v4 BrahmaToolCall.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('renders the database chip and a row-count status chip on success', async () => {
    const fixture = await render({
      success: true,
      sql: 'SELECT id, name FROM characters',
      database: 'main',
      envelope: {
        columns: ['id', 'name'],
        rows: [
          { id: 1, name: 'Ada' },
          { id: 2, name: 'Grace' },
        ],
        rowCount: 2,
        truncated: false,
      },
      errorText: null,
    });
    const body = text(fixture);
    expect(body).toContain('Ran SQL');
    expect(body).toContain('main');
    expect(body).toContain('2 rows');
    // The rows land in a table.
    expect(fixture.nativeElement.querySelectorAll('tbody tr')).toHaveLength(2);
    expect(body).toContain('Ada');
    expect(body).toContain('Grace');
  });

  it('singularizes the row count', async () => {
    const fixture = await render({
      success: true,
      sql: 'SELECT 1',
      database: 'main',
      envelope: { columns: ['n'], rows: [{ n: 1 }], rowCount: 1, truncated: false },
      errorText: null,
    });
    expect(text(fixture)).toContain('1 row');
    expect(text(fixture)).not.toContain('1 rows');
  });

  it('dims a NULL cell', async () => {
    const fixture = await render({
      success: true,
      sql: 'SELECT title FROM chats',
      database: 'main',
      envelope: { columns: ['title'], rows: [{ title: null }], rowCount: 1, truncated: false },
      errorText: null,
    });
    const cell = fixture.nativeElement.querySelector('tbody td') as HTMLElement;
    expect(cell.textContent?.trim()).toBe('NULL');
    expect(cell.classList.contains('qt-text-secondary')).toBe(true);
  });

  it('shows the error text and NO table on failure', async () => {
    const fixture = await render({
      success: false,
      sql: 'DELETE FROM characters',
      database: 'main',
      envelope: null,
      errorText: 'Error: Only read-only queries are permitted.',
    });
    expect(text(fixture)).toContain('Error: Only read-only queries are permitted.');
    expect(text(fixture)).toContain('Failed');
    expect(fixture.nativeElement.querySelector('tbody')).toBeNull();
  });

  it('says so on an empty result', async () => {
    const fixture = await render({
      success: true,
      sql: 'SELECT * FROM chats WHERE 0',
      database: 'main',
      envelope: { columns: ['id'], rows: [], rowCount: 0, truncated: false },
      errorText: null,
    });
    expect(text(fixture)).toContain('No rows returned.');
  });

  it('shows the working indicator while pending', async () => {
    const fixture = await render({
      success: false,
      sql: 'SELECT 1',
      database: 'main',
      envelope: null,
      errorText: null,
      pending: true,
    });
    expect(text(fixture)).toContain('Running…');
    expect(text(fixture)).toContain('Consulting the stacks…');
  });
});
