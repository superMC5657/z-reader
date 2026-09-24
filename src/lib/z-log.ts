// 基于 @tauri-apps/plugin-log 的前端批量日志机制。
//
// 阅读器在刷新订阅时会产生大量高频日志事件，因此通过合并缓冲，
// 达到 FLUSH_MS 时间间隔或 BATCH_SIZE 批量条数时才集中转发，
// 避免每打一行日志就触发一次 IPC 调用。前端和后端的日志都会统一写入
// 以应用名称命名的单一日志文件中（z-reader.log）。

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
    // 异步派发（fire-and-forget）：日志记录绝不能阻塞或破坏前端 UI 流程。
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
    // 页面隐藏/卸载时刷新剩余日志，避免刷新过程中的突发日志在重载或关闭时丢失。
    // 单例注册：enqueue 会在每次打日志时执行，不得重复添加事件监听器。
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
 * 启动批量日志转发器。在应用入口处调用一次。
 * `attachConsole`（webview 控制台日志流）仅在开发模式（DEV）下启用。
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
