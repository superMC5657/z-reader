import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { commandHandlers, lastCall, resetInvoke } from './helpers/invokeHub'
import type { Settings } from '../src/types'
import { useAppStore } from '../src/stores/app'

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

describe('app.ts init 兼容迁移', () => {
  it('j/k 快捷键、compact 视图、缺失的 faviconThirdParty 一次补齐并落盘', async () => {
    const legacy = {
      ...baseSettings(),
      locale: '',
      readerMode: undefined,
      view: 'compact',
      faviconThirdParty: undefined,
      shortcuts: { nextArticle: 'j', prevArticle: 'k' },
    }
    commandHandlers.set('get_settings', async () => legacy)
    const saved: Settings[] = []
    commandHandlers.set('save_settings', async (args: any) => {
      saved.push(args?.settings)
      return undefined
    })

    const store = useAppStore()
    await store.init()

    expect(store.s.shortcuts.nextArticle).toBe('ArrowRight')
    expect(store.s.shortcuts.prevArticle).toBe('ArrowLeft')
    expect(store.s.view).toBe('cards')
    expect(store.s.faviconThirdParty).toBe(true)
    expect(store.s.readerMode).toBe('split')
    expect(store.s.locale).toBeTruthy()
    // 迁移后确实写回了后端（含 locale 首次持久化与快捷键迁移两次）
    expect(saved.length).toBeGreaterThanOrEqual(2)
    const last = lastCall('save_settings')?.args as { settings: Settings } | undefined
    expect(last?.settings.shortcuts.nextArticle).toBe('ArrowRight')
  })

  it('新版配置不再重复写回', async () => {
    commandHandlers.set('get_settings', async () => baseSettings())
    let saves = 0
    commandHandlers.set('save_settings', async () => {
      saves += 1
      return undefined
    })

    const store = useAppStore()
    await store.init()

    expect(store.s.shortcuts.nextArticle).toBe('ArrowRight')
    expect(store.s.view).toBe('cards')
    expect(saves).toBe(0)
  })
})

