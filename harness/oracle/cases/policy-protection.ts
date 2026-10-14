/**
 * Tier-1 oracle — P4.D288 (C1-7): v4's REAL `lib/memory/policy-protection.ts`
 * (bug 201, `af48f6cc2`, at the `83334b9a5` pin) — `policyProtectionOf` over
 * every `(source, tier)` pair a stored row can carry (absent and `null` both
 * read as "not the literal"), and `partitionByPolicyProtection` over mixed
 * corpora (input order kept on both halves; `sparedBy` in v4's key order).
 *
 * Pure: no database, no logger. One NDJSON line per case.
 *
 * Run (Node 24, from the round's pin):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   $N/npx tsx $V5W/harness/oracle/cases/policy-protection.ts > /tmp/oracle-policy-protection.ndjson
 */

async function main(): Promise<void> {
  const { policyProtectionOf, partitionByPolicyProtection } = await import('@/lib/memory/policy-protection');
  const out: string[] = [];
  const SOURCES: Array<string | null | undefined> = ['AUTO', 'MANUAL', 'CONSOLIDATED', 'manual', '', null, undefined];
  const TIERS: Array<string | null | undefined> = ['hot', 'cold', 'COLD', '', null, undefined];
  for (const source of SOURCES) {
    for (const tier of TIERS) {
      const row: Record<string, unknown> = {};
      if (source !== undefined) row.source = source;
      if (tier !== undefined) row.tier = tier;
      out.push(JSON.stringify({ case: 'of', row, result: policyProtectionOf(row as never) }));
    }
  }
  const corpora: Array<Array<Record<string, unknown>>> = [
    [],
    [{ id: 'a', source: 'AUTO', tier: 'hot' }],
    [
      { id: 'm', source: 'MANUAL', tier: 'cold' },
      { id: 'a1', source: 'AUTO', tier: 'hot' },
      { id: 'd', source: 'CONSOLIDATED', tier: 'cold' },
      { id: 'c', source: 'AUTO', tier: 'cold' },
      { id: 'a2', source: 'AUTO' },
      { id: 'm2', source: 'MANUAL' },
      { id: 'n', tier: null },
    ],
  ];
  for (const rows of corpora) {
    const { actionable, spared, sparedBy } = partitionByPolicyProtection(rows as never);
    out.push(
      JSON.stringify({
        case: 'partition',
        rows,
        actionable: (actionable as Array<{ id: string }>).map((r) => r.id),
        spared: (spared as Array<{ id: string }>).map((r) => r.id),
        sparedBy,
      }),
    );
  }
  process.stdout.write(out.join('\n') + '\n');
}

main().catch((err) => {
  process.stderr.write(`policy-protection oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
