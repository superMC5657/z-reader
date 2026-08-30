import { defineStore } from 'pinia'
import { getSettings, saveSettings } from '../lib/tauri'
import type { Settings } from '../types'
import { i18n } from '../i18n'

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
  }),
  getters: {
    s(state): Settings {
      return state.settings ?? DEFAULTS
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
      apply(this.s)
      window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
        if (this.s.theme === 'system') apply(this.s)
      })
    },
    async patch(p: Partial<Settings>) {
      this.settings = { ...this.s, ...p }
      apply(this.s)
      await saveSettings(this.settings)
    },
  },
})
