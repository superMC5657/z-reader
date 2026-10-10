import { vi } from 'vitest'

// happy-dom 没有实现 matchMedia，应用 store 在创建与初始化时会用到，先 stub 掉。
Object.defineProperty(window, 'matchMedia', {
  writable: true,
  value: vi.fn().mockImplementation(() => ({
    matches: false,
    media: '',
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  })),
})
