import { vi } from 'vitest'

export interface InvokeCall {
  cmd: string
  args?: unknown
}

export type CommandHandler = (args?: any) => unknown

/** 记录每一次 invoke 调用，供测试断言。 */
export const invokeCalls: InvokeCall[] = []

/** 按命令名路由的 handler 表，单个测试内按需注册。 */
export const commandHandlers = new Map<string, CommandHandler>()

export const mockInvoke = vi.fn(async (cmd: string, args?: unknown) => {
  invokeCalls.push({ cmd, args })
  const handler = commandHandlers.get(cmd)
  if (!handler) {
    throw new Error(`unmocked invoke: ${cmd}`)
  }
  return handler(args)
})

export function resetInvoke() {
  invokeCalls.length = 0
  commandHandlers.clear()
  mockInvoke.mockClear()
}

export function lastCall(cmd: string): InvokeCall | undefined {
  for (let i = invokeCalls.length - 1; i >= 0; i--) {
    if (invokeCalls[i].cmd === cmd) return invokeCalls[i]
  }
  return undefined
}

/**
 * fake-invoke 适配缝的统一断言入口：回滚/reload 交错断言一律经此取调用，
 * 不直接数 mock 内部状态。
 */
export function callsFor(cmd: string): InvokeCall[] {
  return invokeCalls.filter((c) => c.cmd === cmd)
}

/** 可控门闩：编排 optimistic→queue→reload 交错时序的唯一原语。 */
export function gate<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason?: unknown) => void
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}
