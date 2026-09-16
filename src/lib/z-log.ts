// Batched frontend logging via @tauri-apps/plugin-log.
//
// The reader fires high-volume events during subscription refreshes, so
// records are coalesced and forwarded at most every FLUSH_MS or BATCH_SIZE
// entries instead of one IPC call per log line. Both frontend and backend
// records land in a single unified log file named after the application (z-reader.log).

import { attachConsole, debug, error, info, trace, warn } from '@tauri-apps/plugin-log'

type Level = 'trace' | 'debug' | 'info' | 'warn' | 'error'

interface Entry {
  level: Level
  message: string
}

const BATCH_SIZE = 200
const FLUSH_MS = 200

const senders: Record<Level, (message: string) => Promise<void>> = {
  trace,
  debug,
  info,
  warn,
  error,
}

const queue: Entry[] = []
let timer: number | undefined
let started = false
let consoleAttached = false
let pagehideHooked = false

function flush(): void {
  if (timer !== undefined) {
    clearInterval(timer)
    timer = undefined
  }
  if (queue.length === 0) return
  const batch = queue.splice(0, queue.length)
  for (const entry of batch) {
    // Fire-and-forget: logging must never break UI flows.
    senders[entry.level](entry.message).catch(() => undefined)
  }
}

function schedule(): void {
  if (timer !== undefined) return
  timer = window.setInterval(flush, FLUSH_MS)
}

function enqueue(level: Level, message: string, immediate = false): void {
  queue.push({ level, message })
  if (immediate || queue.length >= BATCH_SIZE) {
    flush()
  } else {
    schedule()
  }
  if (typeof window !== 'undefined' && !pagehideHooked) {
    // Flush remaining entries when the page hides so refresh bursts
    // are not lost on reload/close. Registered once: enqueue runs per
    // log line and must not pile up duplicate listeners.
    pagehideHooked = true
    window.addEventListener('pagehide', flush)
  }
}

function formatAction(action: string, meta?: Record<string, unknown>): string {
  if (!meta || Object.keys(meta).length === 0) {
    return action
  }
  const pairs = Object.entries(meta)
    .filter(([_, v]) => v !== undefined && v !== null)
    .map(([k, v]) => `${k}=${typeof v === 'string' && v.includes(' ') ? `"${v}"` : v}`)
    .join(' ')
  return `${action} (${pairs})`
}

/**
 * Start the batched forwarder. Call once from the app entry.
 * `attachConsole` (webview log streaming) is DEV-only.
 */
export function initZLog(): void {
  if (started) return
  started = true
  schedule()
  if (import.meta.env.DEV && !consoleAttached) {
    consoleAttached = true
    attachConsole().catch(() => undefined)
  }
  if (typeof window !== 'undefined') {
    window.addEventListener('error', (event) => {
      zlog.error(`uncaught error: ${event.message} at ${event.filename}:${event.lineno}`)
    })
    window.addEventListener('unhandledrejection', (event) => {
      zlog.error(`unhandled rejection: ${event.reason}`)
    })
  }
}

export const zlog = {
  trace: (message: string): void => enqueue('trace', message),
  debug: (message: string): void => enqueue('debug', message),
  info: (message: string): void => enqueue('info', message),
  warn: (message: string): void => enqueue('warn', message, true),
  error: (message: string): void => enqueue('error', message, true),
  ui: (action: string, meta?: Record<string, unknown>): void =>
    enqueue('info', formatAction(action, meta), true),
  uiDebug: (action: string, meta?: Record<string, unknown>): void =>
    enqueue('debug', formatAction(action, meta)),
  flush,
}
