import type { InjectionKey } from 'vue'

export interface ConfirmOptions {
  title?: string
  message: string
  confirmText?: string
  danger?: boolean
  onConfirm: () => void | Promise<void>
}

/** 薄壳持有的全局确认框触发器，各 tab 注入使用，保持确认框 DOM 与行为不变。 */
export const requestConfirmKey: InjectionKey<(options: ConfirmOptions) => void> =
  Symbol('settingsRequestConfirm')
