import { defineStore } from 'pinia'
import { getSettings, saveSettings } from '../lib/tauri'
import type { Settings } from '../types'
import { i18n } from '../i18n'
import { zlog } from '../lib/z-log'

export const DEFAULT_SHORTCUTS: Record<string, string> = {
  nextArticle: 'ArrowRight',
  prevArticle: 'ArrowLeft',
  toggleRead: 'm',
  toggleStar: 's',
  fetchFull: 'f',
  openInBrowser: 'o',
  refresh: 'r',
  closeArticle: 'Escape',
  addSource: 'a',
  toggleSidebar: 'b',
}

const DEFAULTS: Settings = {
  version: '0.1.0',
  theme: 'system',
  view: 'cards',
  locale: navigator.language.toLowerCase().startsWith('zh') ? 'zh-CN' : 'en-US',
  uiScale: 100,
  fontSize: 16,
  fetchInterval: 30,
  filterType: 0,
  viewConfigs: 0b111,
  menuOn: true,
  readerMode: 'split',
  shortcuts: DEFAULT_SHORTCUTS,
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

export async function setAppZoom(scaleFactor: number) {
  try {
    if (typeof window !== 'undefined' && (window as any).__TAURI_INTERNALS__) {
      const { getCurrentWebview } = await import('@tauri-apps/api/webview')
      const webview = getCurrentWebview()
      await webview.setZoom(scaleFactor)
      ;(document.documentElement.style as any).zoom = ''
      return
    }
  } catch {
  }
  if (typeof document !== 'undefined') {
    ;(document.documentElement.style as any).zoom = `${scaleFactor}`
  }
}

function resolveTheme(theme: Settings['theme']): 'light' | 'dark' {
  if (theme === 'system') {
    return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
  }
  return theme
}

/**
 * settings validate+confirm+persist 的 validate 半区（纯函数，无 IO）。
 *
 * 不变式：任何 tab/调用方传进来的 patch 都先经此归一，非法值直接丢弃、
 * 绝不落盘。tab 只做薄适配传原值（trim/clamp/枚举收敛一律在这里），
 * 确认（confirm）由 SettingsPanel 的全局 ConfirmModal 拥有，
 * 持久化（persist）由 patch 经 tauri 适配层完成。
 * validate 在 store、confirm 在 panel、persist 经 adapter：三者各就其位。
 */
export const SETTINGS_RANGES = {
  uiScale: { min: 80, max: 150 },
  fontSize: { min: 12, max: 22 },
} as const

const THEMES: Settings['theme'][] = ['system', 'light', 'dark']
const VIEWS: Settings['view'][] = ['cards', 'magazine', 'list']
const READER_MODES: Settings['readerMode'][] = ['split', 'focus']
const PROXY_MODES: Settings['proxyMode'][] = ['system', 'none', 'manual']
const FILTER_TYPES = [0, 1, 2]
const BOOLEAN_KEYS = ['menuOn', 'notifyOnNew', 'closeToTray', 'faviconThirdParty'] as const

function clampInt(n: unknown, min: number, max: number): number | undefined {
  if (typeof n !== 'number' || !Number.isFinite(n)) return undefined
  return Math.min(max, Math.max(min, Math.round(n)))
}

/** 与原 AppTab 内联语义逐值等价：空/NaN/0 回落 30，下限钳到 1。 */
function normalizeFetchInterval(v: unknown): number {
  const raw = Number(v)
  if (!Number.isFinite(raw) || raw === 0) return 30
  return Math.max(1, raw)
}

export function normalizeSettingsPatch(p: Partial<Settings>): Partial<Settings> {
  const out: Partial<Settings> = {}
  if (p.theme !== undefined && THEMES.includes(p.theme)) out.theme = p.theme
  if (p.view !== undefined) {
    if (VIEWS.includes(p.view)) out.view = p.view
    else if ((p.view as string) === 'compact') out.view = 'cards'
  }
  if (p.locale !== undefined && typeof p.locale === 'string' && p.locale.trim()) {
    out.locale = p.locale.trim()
  }
  if (p.uiScale !== undefined) {
    const v = clampInt(p.uiScale, SETTINGS_RANGES.uiScale.min, SETTINGS_RANGES.uiScale.max)
    if (v !== undefined) out.uiScale = v
  }
  if (p.fontSize !== undefined) {
    const v = clampInt(p.fontSize, SETTINGS_RANGES.fontSize.min, SETTINGS_RANGES.fontSize.max)
    if (v !== undefined) out.fontSize = v
  }
  if (p.fetchInterval !== undefined) out.fetchInterval = normalizeFetchInterval(p.fetchInterval)
  if (p.filterType !== undefined && FILTER_TYPES.includes(p.filterType)) {
    out.filterType = p.filterType
  }
  if (p.viewConfigs !== undefined && typeof p.viewConfigs === 'number' && Number.isFinite(p.viewConfigs)) {
    out.viewConfigs = Math.trunc(p.viewConfigs)
  }
  for (const k of BOOLEAN_KEYS) {
    if (p[k] !== undefined && typeof p[k] === 'boolean') {
      ;(out as Record<string, boolean>)[k] = p[k] as boolean
    }
  }
  if (p.readerMode !== undefined && READER_MODES.includes(p.readerMode)) {
    out.readerMode = p.readerMode
  }
  if (p.shortcuts !== undefined && typeof p.shortcuts === 'object' && p.shortcuts !== null) {
    const kept: Record<string, string> = {}
    // 空字符串视为未设置：直接丢弃该键，读取时回落到 DEFAULT_SHORTCUTS。
    for (const [k, v] of Object.entries(p.shortcuts)) {
      if (typeof v === 'string' && v) kept[k] = v
    }
    out.shortcuts = kept
  }
  if (p.proxyMode !== undefined && PROXY_MODES.includes(p.proxyMode)) out.proxyMode = p.proxyMode
  if (p.proxyUrl !== undefined && typeof p.proxyUrl === 'string') out.proxyUrl = p.proxyUrl.trim()
  if (p.proxyUsername !== undefined && typeof p.proxyUsername === 'string') {
    out.proxyUsername = p.proxyUsername
  }
  if (p.proxyPassword !== undefined && typeof p.proxyPassword === 'string') {
    out.proxyPassword = p.proxyPassword
  }
  if (p.retentionDays !== undefined && typeof p.retentionDays === 'number' && Number.isFinite(p.retentionDays)) {
    out.retentionDays = Math.max(0, Math.floor(p.retentionDays))
  }
  if (p.maxItemsPerSource !== undefined && typeof p.maxItemsPerSource === 'number' && Number.isFinite(p.maxItemsPerSource)) {
    out.maxItemsPerSource = Math.max(0, Math.floor(p.maxItemsPerSource))
  }
  if (p.syncAccount !== undefined && (p.syncAccount === null || typeof p.syncAccount === 'object')) {
    out.syncAccount = p.syncAccount
  }
  if (p.version !== undefined && typeof p.version === 'string' && p.version) {
    out.version = p.version
  }
  return out
}

/** 同步登录表单的形状校验（与原 SyncTab 内联判定逐值等价，文案 key 不变）。 */
export function validateSyncLoginInput(input: {
  serverUrl: string
  username: string
  password: string
}): boolean {
  return Boolean(input.serverUrl.trim() && input.username.trim() && input.password)
}

function apply(s: Settings) {
  document.documentElement.dataset.theme = resolveTheme(s.theme)
  document.documentElement.style.setProperty('--app-font-size', `${s.fontSize}px`)
  i18n.global.locale.value = s.locale as 'zh-CN' | 'en-US'
  setAppZoom((s.uiScale || 100) / 100)
}

export const useAppStore = defineStore('app', {
  state: () => ({
    settings: null as Settings | null,
    systemDark: window.matchMedia('(prefers-color-scheme: dark)').matches,
  }),
  getters: {
    s(state): Settings {
      return state.settings ?? DEFAULTS
    },
    isDark(state): boolean {
      const theme = (state.settings ?? DEFAULTS).theme
      if (theme === 'system') {
        return state.systemDark
      }
      return theme === 'dark'
    },
    isFocusMode(state): boolean {
      return (state.settings ?? DEFAULTS).readerMode === 'focus'
    },
    shortcuts(state): Record<string, string> {
      return { ...DEFAULT_SHORTCUTS, ...(state.settings?.shortcuts ?? {}) }
    },
    showCover(): boolean {
      return (this.s.viewConfigs & 1) !== 0
    },
    showSnippet(): boolean {
      return (this.s.viewConfigs & 2) !== 0
    },
    fadeRead(): boolean {
      return (this.s.viewConfigs & 4) !== 0
    },
  },
  actions: {
    async init() {
      this.settings = await getSettings().catch(() => DEFAULTS)
      if (!this.s.locale) {
        // 首次运行：读取系统语言并持久化保存。
        this.settings.locale = DEFAULTS.locale
        await saveSettings(this.settings).catch(() => {})
      }
      if (!this.s.shortcuts || Object.keys(this.s.shortcuts).length === 0) {
        this.settings.shortcuts = { ...DEFAULT_SHORTCUTS }
      } else {
        let changed = false
        if (this.settings.shortcuts.nextArticle === 'j') {
          this.settings.shortcuts.nextArticle = 'ArrowRight'
          changed = true
        }
        if (this.settings.shortcuts.prevArticle === 'k') {
          this.settings.shortcuts.prevArticle = 'ArrowLeft'
          changed = true
        }
        if (changed) {
          await saveSettings(this.settings).catch(() => {})
        }
      }
      if (!this.s.readerMode) {
        this.settings.readerMode = 'split'
      }
      if (this.s.faviconThirdParty === undefined) {
        // 兼容该设置项引入前旧版本保存的配置文件。
        this.settings.faviconThirdParty = true
      }
      if ((this.s.view as string) === 'compact') {
        this.settings.view = 'cards'
      }
      apply(this.s)
      const mql = window.matchMedia('(prefers-color-scheme: dark)')
      mql.addEventListener('change', (e) => {
        this.systemDark = e.matches
        if (this.s.theme === 'system') apply(this.s)
      })
    },
    async toggleFocusMode() {
      const next = this.s.readerMode === 'focus' ? 'split' : 'focus'
      zlog.ui(`Switch reader mode to "${next}"`)
      await this.patch({ readerMode: next })
    },
    async setShortcut(actionKey: string, key: string) {
      const shortcuts = { ...this.shortcuts, [actionKey]: key }
      await this.patch({ shortcuts })
    },
    async resetShortcuts() {
      await this.patch({ shortcuts: { ...DEFAULT_SHORTCUTS } })
    },
    async patch(p: Partial<Settings>) {
      const clean = normalizeSettingsPatch(p)
      // 全非法补丁直接丢弃：不变式要求非法值绝不触达 apply 与落盘。
      if (Object.keys(clean).length === 0) return
      zlog.ui(`Update settings: ${Object.keys(clean).join(', ')}`)
      this.settings = { ...this.s, ...clean }
      apply(this.s)
      await saveSettings(this.settings)
    },
    /** 后端发生变更后（如同步登录/登出）重新读取 settings.json。 */
    async refresh() {
      this.settings = await getSettings().catch(() => this.settings)
      if (this.settings) apply(this.s)
    },
  },
})
