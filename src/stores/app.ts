import { defineStore } from 'pinia'
import { getSettings, saveSettings } from '../lib/tauri'
import type { Settings } from '../types'
import { i18n } from '../i18n'

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
}

function resolveTheme(theme: Settings['theme']): 'light' | 'dark' {
  if (theme === 'system') {
    return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
  }
  return theme
}

function apply(s: Settings) {
  document.documentElement.dataset.theme = resolveTheme(s.theme)
  document.documentElement.style.setProperty('--app-font-size', `${s.fontSize}px`)
  i18n.global.locale.value = s.locale as 'zh-CN' | 'en-US'
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
        // First run: pick the system locale and persist it.
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
      this.settings = { ...this.s, ...p }
      apply(this.s)
      await saveSettings(this.settings)
    },
  },
})
