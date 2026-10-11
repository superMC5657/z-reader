import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { callsFor, commandHandlers, lastCall, resetInvoke } from './helpers/invokeHub'
import type { Settings } from '../src/types'
import {
  normalizeSettingsPatch,
  useAppStore,
  validateSyncLoginInput,
} from '../src/stores/app'
import { requestDestructiveConfirm } from '../src/components/settings/settingsConfirm'

vi.mock('@tauri-apps/api/core', async () => {
  const hub = await import('./helpers/invokeHub')
  return { invoke: hub.mockInvoke }
})

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
}))

vi.mock('@tauri-apps/plugin-log', () => ({
  attachConsole: vi.fn(async () => {}),
  trace: vi.fn(async () => {}),
  debug: vi.fn(async () => {}),
  info: vi.fn(async () => {}),
  warn: vi.fn(async () => {}),
  error: vi.fn(async () => {}),
}))

function baseSettings(): Settings {
  return {
    version: '0.1.0',
    theme: 'system',
    view: 'cards',
    locale: 'en-US',
    uiScale: 100,
    fontSize: 16,
    fetchInterval: 30,
    filterType: 0,
    viewConfigs: 7,
    menuOn: true,
    readerMode: 'split',
    shortcuts: { nextArticle: 'ArrowRight' },
    proxyMode: 'system',
    proxyUrl: '',
    proxyUsername: '',
    proxyPassword: '',
    notifyOnNew: true,
    closeToTray: true,
    retentionDays: 0,
    maxItemsPerSource: 0,
    faviconThirdParty: true,
    syncAccount: null,
  }
}

beforeEach(() => {
  setActivePinia(createPinia())
  resetInvoke()
})

describe('normalizeSettingsPatch：validate 半区收敛（与原 tab 内联语义等价）', () => {
  it('uiScale/fontSize 钳位，fetchInterval 空/NaN 回落 30，下限 1', () => {
    const base = baseSettings()
    expect(normalizeSettingsPatch({ uiScale: 999 }, base).uiScale).toBe(150)
    expect(normalizeSettingsPatch({ uiScale: 10 }, base).uiScale).toBe(80)
    expect(normalizeSettingsPatch({ fontSize: 99 }, base).fontSize).toBe(22)
    expect(normalizeSettingsPatch({ fontSize: 1 }, base).fontSize).toBe(12)
    expect(normalizeSettingsPatch({ fetchInterval: Number('') }, base).fetchInterval).toBe(30)
    expect(normalizeSettingsPatch({ fetchInterval: Number('abc') }, base).fetchInterval).toBe(30)
    expect(normalizeSettingsPatch({ fetchInterval: -5 }, base).fetchInterval).toBe(1)
    expect(normalizeSettingsPatch({ fetchInterval: 60 }, base).fetchInterval).toBe(60)
  })

  it('非法枚举/类型直接丢弃，proxyUrl 收 trim，legacy compact 收 cards', () => {
    const base = baseSettings()
    expect(normalizeSettingsPatch({ theme: 'nope' as never }, base).theme).toBeUndefined()
    expect(normalizeSettingsPatch({ filterType: 3 }, base).filterType).toBeUndefined()
    expect(normalizeSettingsPatch({ proxyUrl: '  http://127.0.0.1:7890  ' }, base).proxyUrl).toBe(
      'http://127.0.0.1:7890',
    )
    expect(normalizeSettingsPatch({ view: 'compact' as never }, base).view).toBe('cards')
    expect(
      normalizeSettingsPatch({ shortcuts: { a: '', b: 'x' } }, base).shortcuts,
    ).toEqual({ b: 'x' })
  })
})

describe('patch：validate+confirm+persist 经统一入口', () => {
  it('越界 uiScale 落盘钳位值；全非法补丁不触达 persist', async () => {
    commandHandlers.set('get_settings', async () => baseSettings())
    const saved: Settings[] = []
    commandHandlers.set('save_settings', async (args: any) => {
      saved.push(args?.settings)
      return undefined
    })

    const store = useAppStore()
    await store.init()

    await store.patch({ uiScale: 999 })
    expect(store.s.uiScale).toBe(150)
    expect(lastCall('save_settings')?.args).toMatchObject({ settings: { uiScale: 150 } })

    const savesBefore = saved.length
    await store.patch({ theme: 'nope' as never })
    expect(store.s.theme).toBe('system')
    expect(saved.length).toBe(savesBefore)
    expect(callsFor('save_settings').length).toBe(savesBefore)
  })
})

describe('validateSyncLoginInput：与原 SyncTab 内联判定等价', () => {
  it('三空任一即 false，齐备即 true', () => {
    expect(
      validateSyncLoginInput({ serverUrl: '  ', username: 'u', password: 'p' }),
    ).toBe(false)
    expect(validateSyncLoginInput({ serverUrl: 'https://x', username: ' ', password: 'p' })).toBe(
      false,
    )
    expect(validateSyncLoginInput({ serverUrl: 'https://x', username: 'u', password: '' })).toBe(
      false,
    )
    expect(
      validateSyncLoginInput({ serverUrl: 'https://x', username: 'u', password: 'p' }),
    ).toBe(true)
  })
})

describe('requestDestructiveConfirm：confirm 半区唯一调用面', () => {
  it('danger 默认 true，显式覆盖被尊重，回调原样透传', () => {
    const sink = vi.fn()
    const onConfirm = vi.fn()
    requestDestructiveConfirm(sink, { title: 't', message: 'm', onConfirm })
    expect(sink).toHaveBeenCalledTimes(1)
    expect(sink.mock.calls[0][0]).toMatchObject({ title: 't', message: 'm', danger: true })
    expect(sink.mock.calls[0][0].onConfirm).toBe(onConfirm)

    sink.mockClear()
    requestDestructiveConfirm(sink, { message: 'm', danger: false, onConfirm })
    expect(sink.mock.calls[0][0].danger).toBe(false)
  })
})
