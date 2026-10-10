import { describe, expect, it, vi } from 'vitest'
import {
  createShortcutRecorder,
  formatKeyDisplay,
  normalizeRecordedKey,
} from '../src/components/settings/shortcutRecorder'

function fakeKey(key: string) {
  return {
    key,
    preventDefault: vi.fn(),
    stopPropagation: vi.fn(),
  }
}

describe('快捷键录制归一', () => {
  it('空格记 Space，单字符转小写，功能键保持原样', () => {
    expect(normalizeRecordedKey(' ')).toBe('Space')
    expect(normalizeRecordedKey('J')).toBe('j')
    expect(normalizeRecordedKey('m')).toBe('m')
    expect(normalizeRecordedKey('ArrowUp')).toBe('ArrowUp')
    expect(normalizeRecordedKey('Escape')).toBe('Escape')
    expect(normalizeRecordedKey('Enter')).toBe('Enter')
  })

  it('按键展示与线上一致', () => {
    expect(formatKeyDisplay(undefined)).toBe('—')
    expect(formatKeyDisplay('Escape')).toBe('⎋ Esc')
    expect(formatKeyDisplay(' ')).toBe('␣ Space')
    expect(formatKeyDisplay('ArrowLeft')).toBe('←')
    expect(formatKeyDisplay('m')).toBe('M')
  })
})

describe('录制状态机', () => {
  it('未进入录制时按键直接忽略，不回调查找', () => {
    const onCommit = vi.fn()
    const recorder = createShortcutRecorder(onCommit)
    const e = fakeKey('j')
    recorder.onRecordKeydown(e)
    expect(onCommit).not.toHaveBeenCalled()
  })

  it('录制中按下按键：归一后回调查找并退出录制态', () => {
    const onCommit = vi.fn()
    const recorder = createShortcutRecorder(onCommit)
    recorder.recordingAction.value = 'nextArticle'

    const e = fakeKey('j')
    recorder.onRecordKeydown(e)

    expect(e.preventDefault).toHaveBeenCalled()
    expect(e.stopPropagation).toHaveBeenCalled()
    expect(onCommit).toHaveBeenCalledWith('nextArticle', 'j')
    expect(recorder.recordingAction.value).toBeNull()
  })

  it('录制空格得到 Space，可接着录下一个动作', () => {
    const onCommit = vi.fn()
    const recorder = createShortcutRecorder(onCommit)
    recorder.recordingAction.value = 'toggleRead'
    recorder.onRecordKeydown(fakeKey(' '))

    expect(onCommit).toHaveBeenCalledWith('toggleRead', 'Space')

    recorder.recordingAction.value = 'refresh'
    recorder.onRecordKeydown(fakeKey('R'))
    expect(onCommit).toHaveBeenCalledWith('refresh', 'r')
  })
})
