import { ref } from 'vue'

/**
 * 快捷键录制的薄适配层：只拥有按键归一（normalize）与录制态，
 * 提交一律经 app.setShortcut 进入 settings store 的 validate+confirm+persist，
 * 空键等非法值由 store 的 normalize 丢弃，这里不自建校验副本。
 */

/** 录制时按键名的归一规则：空格记为 Space，单个字符统一小写，其余保持原样。 */
export function normalizeRecordedKey(key: string): string {
  if (key === ' ') return 'Space'
  if (key.length === 1) return key.toLowerCase()
  return key
}

export function formatKeyDisplay(k?: string): string {
  if (!k) return '—'
  if (k === 'Escape') return '⎋ Esc'
  if (k === 'Space' || k === ' ') return '␣ Space'
  if (k === 'ArrowUp') return '↑'
  if (k === 'ArrowDown') return '↓'
  if (k === 'ArrowLeft') return '←'
  if (k === 'ArrowRight') return '→'
  if (k === 'Enter') return '↵ Enter'
  return k.toUpperCase()
}

export interface RecordableKeyEvent {
  key: string
  preventDefault(): void
  stopPropagation(): void
}

/**
 * 快捷键录制状态机：start 进入录制并注册一次性全局监听，
 * onRecordKeydown 归一化后回调查找调用方落盘，自己退出录制态。
 */
export function createShortcutRecorder(onCommit: (actionKey: string, key: string) => void) {
  const recordingAction = ref<string | null>(null)

  function onRecordKeydown(e: RecordableKeyEvent) {
    e.preventDefault()
    e.stopPropagation()
    if (!recordingAction.value) return
    onCommit(recordingAction.value, normalizeRecordedKey(e.key))
    recordingAction.value = null
  }

  function start(actionKey: string) {
    recordingAction.value = actionKey
    window.addEventListener('keydown', onRecordKeydown as (e: KeyboardEvent) => void, {
      capture: true,
      once: true,
    })
  }

  return { recordingAction, start, onRecordKeydown }
}
