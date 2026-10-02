/**
 * Reading the database's timestamps as the instants they are.
 *
 * SQLite's `datetime('now')`, which stamps every chat and message row, writes
 * UTC with no zone: `2026-10-01 08:00:00`. So does the model detail's
 * `%Y-%m-%d %H:%M:%S`. `new Date` reads a date-time with no zone as *local*
 * time, so each of those showed off by the zone's offset — ten hours in
 * Brisbane. This reads one as the UTC it is.
 *
 * Text that names its zone (`Z` or an offset), a bare date, or anything else
 * not shaped like that goes to `new Date` unchanged.
 *
 * @module utils/dbTimestamp
 */

/** A date and a time, with no zone after it. */
const ZONELESS = /^(\d{4}-\d{2}-\d{2})[ T](\d{2}:\d{2}(?::\d{2}(?:\.\d+)?)?)$/;

/** Parse a timestamp from the database, reading one with no zone as UTC. */
export function parseDbTimestamp(text: string): Date {
  const zoneless = ZONELESS.exec(text.trim());
  return new Date(zoneless ? `${zoneless[1]}T${zoneless[2]}Z` : text);
}
