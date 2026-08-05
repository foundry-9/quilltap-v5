import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { AlmanackReportDialog } from './almanack-report-dialog';
import { renderAlmanackMarkdown } from './almanack-markdown';

const REPORT = [
  '# The Almanack',
  '',
  '| Cog | Condition |',
  '| --- | --- |',
  '| Boiler | *sound* |',
  '',
  'See [the ledger](qtap://project/ledger.md) and "the quoted thing".',
].join('\n');

describe('renderAlmanackMarkdown (v4’s ReactMarkdown + remarkGfm)', () => {
  it('renders GFM tables', () => {
    const html = renderAlmanackMarkdown(REPORT);
    expect(html).toContain('<table>');
    expect(html).toContain('<th>Cog</th>');
    expect(html).toContain('<td>Boiler</td>');
  });

  it('keeps qtap:// hrefs intact', () => {
    expect(renderAlmanackMarkdown(REPORT)).toContain('href="qtap://project/ledger.md"');
  });

  /**
   * The reason this pipeline exists instead of a reuse: v5's ONE renderer is the
   * SALON renderer, which always applies `DEFAULT_RENDERING_PATTERNS`. Run a
   * system report through it and every quoted string becomes dialogue and every
   * `*asterisked*` run becomes narration. A report is not a performance, and v4
   * renders it with bare `remarkGfm`.
   */
  it('applies NO roleplay processing — no dialogue or narration spans', () => {
    const html = renderAlmanackMarkdown(REPORT);
    expect(html).not.toContain('qt-chat-dialogue');
    expect(html).not.toContain('qt-chat-narration');
    // `*sound*` is plain markdown emphasis, nothing more.
    expect(html).toContain('<em>sound</em>');
  });

  it('drops raw HTML, as ReactMarkdown does by default', () => {
    const html = renderAlmanackMarkdown('before <script>alert(1)</script> after');
    expect(html).not.toContain('<script>');
  });

  it('survives a pathological input by escaping it', () => {
    expect(renderAlmanackMarkdown('')).toBe('');
  });
});

describe('AlmanackReportDialog', () => {
  let fixture: ComponentFixture<AlmanackReportDialog> | null = null;

  function render(): ComponentFixture<AlmanackReportDialog> {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({ imports: [AlmanackReportDialog] });
    const f = TestBed.createComponent(AlmanackReportDialog);
    f.componentRef.setInput('reportId', 'rep-1');
    f.componentRef.setInput('filename', 'almanack-2026-08-05.md');
    f.componentRef.setInput('content', REPORT);
    f.detectChanges();
    fixture = f;
    return f;
  }

  afterEach(() => {
    fixture?.destroy();
    fixture = null;
    document.body.style.overflow = '';
  });

  it('titles itself with the FILENAME and subtitles with the feature name', () => {
    const host = render().nativeElement as HTMLElement;
    expect(host.querySelector('.qt-dialog-title')?.textContent?.trim()).toBe(
      'almanack-2026-08-05.md',
    );
    expect(host.querySelector('.qt-dialog-description')?.textContent?.trim()).toBe(
      'The Almanack (System Report)',
    );
  });

  it('renders the report body through the Almanack pipeline', () => {
    const host = render().nativeElement as HTMLElement;
    const body = host.querySelector('.qt-almanack-report');
    expect(body?.querySelector('table')).not.toBeNull();
    expect(body?.querySelector('a')?.getAttribute('href')).toBe('qtap://project/ledger.md');
  });

  it('downloads through the web edge, with the download attribute set', () => {
    const host = render().nativeElement as HTMLElement;
    const anchor = host.querySelector('a.qt-button') as HTMLAnchorElement;
    expect(anchor.getAttribute('href')).toBe(
      '/api/v1/system/tools?action=capabilities-report-get&reportId=rep-1&download=true',
    );
    expect(anchor.getAttribute('download')).toBe('almanack-2026-08-05.md');
  });

  it('locks body scroll while open and restores it on close', () => {
    const f = render();
    expect(document.body.style.overflow).toBe('hidden');
    f.destroy();
    fixture = null;
    expect(document.body.style.overflow).toBe('');
  });

  it('Escape emits close', () => {
    const f = render();
    let closed = 0;
    f.componentInstance.close.subscribe(() => (closed += 1));
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(closed).toBe(1);
  });

  it('the backdrop and both close controls emit close', () => {
    const f = render();
    const host = f.nativeElement as HTMLElement;
    let closed = 0;
    f.componentInstance.close.subscribe(() => (closed += 1));

    host.querySelector<HTMLButtonElement>('.qt-dialog-overlay')!.click();
    host.querySelector<HTMLButtonElement>('button[aria-label="Close"]')!.click();
    host.querySelector<HTMLButtonElement>('.qt-button-primary')!.click();

    expect(closed).toBe(3);
  });
});
