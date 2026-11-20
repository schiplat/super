import { format, isToday, isYesterday } from 'date-fns'
import type { ProgramEventRecord } from '@/types'

export type EventTone = 'neutral' | 'success' | 'warning' | 'danger'

/**
 * Severity is a property of the event, not of the page: an exit with a non-zero
 * code is always dangerous, a restart/backoff is a warning, and the quiet
 * lifecycle events stay neutral. Names are matched, not enumerated, so new
 * event types from the daemon still land in a sensible bucket.
 */
export function eventTone(event: string, exitCode?: number | null): EventTone {
  const name = event.toLowerCase()
  if (exitCode != null && exitCode !== 0) return 'danger'
  if (
    name.includes('crash') ||
    name.includes('fatal') ||
    name.includes('fail') ||
    name.includes('error') ||
    name.includes('oom')
  ) {
    return 'danger'
  }
  if (
    name.includes('restart') ||
    name.includes('backoff') ||
    name.includes('kill') ||
    name.includes('warn') ||
    name.includes('pressure')
  ) {
    return 'warning'
  }
  if (
    name.includes('start') ||
    name.includes('healthy') ||
    name.includes('stop') ||
    name.includes('exit') ||
    name.includes('recovered')
  ) {
    return 'success'
  }
  return 'neutral'
}

/** `HH:mm:ss` inside a day group. */
export function eventTime(ts: number): string {
  return ts ? format(new Date(ts * 1000), 'HH:mm:ss') : '—'
}

/** Full timestamp for the expanded detail. */
export function eventTimestamp(ts: number): string {
  return ts ? format(new Date(ts * 1000), 'yyyy-MM-dd HH:mm:ss') : '—'
}

/** Day-group label: Today / Yesterday / `MMM d`. */
export function eventDayLabel(ts: number): string {
  if (!ts) return 'Unknown date'
  const date = new Date(ts * 1000)
  if (isToday(date)) return 'Today'
  if (isYesterday(date)) return 'Yesterday'
  return format(date, 'MMM d')
}

/** `exit 1` / `sig 11` / em dash. */
export function eventResult(row: ProgramEventRecord): string {
  if (row.exit_code != null) return `exit ${row.exit_code}`
  if (row.signal != null) return `sig ${row.signal}`
  return '—'
}
