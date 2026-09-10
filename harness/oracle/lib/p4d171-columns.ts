/**
 * The v4-side heal for the two `78b381a96` schema moves (P4.D172).
 *
 * Several committed test fixtures (`salon-main.db` and friends) predate
 * `chat_messages.routeTrail` and `chats.cycleOrderParticipantIds`. v4's routes at
 * that tip WRITE both — `handleTurnAction` writes the rotation on every call — so
 * an oracle run over an un-migrated fixture copy dies with `no such column`
 * before any case executes, and the diff then reads like a port defect.
 *
 * This is the v4-side twin of v5's `test_support::ensure_p4d171_columns` (the
 * repaired-at-boot idiom P4.D171 introduced for exactly these fixtures).
 * Idempotent, and deliberately NOT a fixture regen: the committed pairs are read
 * by other families, and regenerating one to satisfy another is how a fixture
 * stops being comparable to its siblings.
 *
 * Call AFTER `initializeDatabase()`, on the copy, before any case runs.
 */
export function ensureP4D171Columns(raw: {
  prepare: (sql: string) => { all: () => unknown[] };
  exec: (sql: string) => unknown;
}): void {
  const has = (table: string, column: string) =>
    (raw.prepare(`PRAGMA table_info(${table})`).all() as Array<{ name: string }>).some(
      (c) => c.name === column,
    );
  if (!has('chats', 'cycleOrderParticipantIds')) {
    raw.exec("ALTER TABLE chats ADD COLUMN cycleOrderParticipantIds TEXT DEFAULT '[]'");
  }
  if (!has('chat_messages', 'routeTrail')) {
    raw.exec('ALTER TABLE chat_messages ADD COLUMN routeTrail TEXT');
  }
}
