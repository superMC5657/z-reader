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

/**
 * 危险确认在设置面板内的统一适配器：面板内的破坏性操作（删源/删规则/
 * 断开同步/恢复备份）一律经此走 SettingsPanel 的全局 ConfirmModal，
 * danger 默认 true；各 tab 只传文本与回调，不自建弹框、不各自维护
 * visible/loading 副本。注：SideNav 自有的 confirmState 是历史并存实现，
 * 不在本轮收敛范围。
 */
export function requestDestructiveConfirm(
  requestConfirm: (options: ConfirmOptions) => void,
  options: ConfirmOptions,
): void {
  requestConfirm({ danger: true, ...options })
}
