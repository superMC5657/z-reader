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
