import { beforeEach, describe, expect, it } from 'vitest'
import { defineComponent } from 'vue'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { i18n } from '../src/i18n/index'
import SettingsPanel from '../src/components/SettingsPanel.vue'
import AddSourceDialog from '../src/components/AddSourceDialog.vue'
import Modal from '../src/components/ui/Modal.vue'

const mountShell = (component: unknown, extra: Record<string, unknown> = {}) =>
  mount(component as never, {
    attachTo: document.body,
    global: { plugins: [i18n], stubs: { transition: false } },
    ...extra,
  })

beforeEach(() => {
  setActivePinia(createPinia())
  document.body.innerHTML = ''
})

describe('modal open chain (regression: dialogs must render on first open)', () => {
  // 2026-10-10 还债：show 机制已删除，Modal 无条件渲染，显隐一律由调用方
  // 外层 v-if 控制（ConfirmModal 四处调用方均已改 v-if）。
  // 以下用例把"挂载即有 .modal-mask"锁死（内容经 Teleport 搬到 body）。
  it('settings panel renders mask, sidebar and default sources tab', () => {
    const wrapper = mountShell(SettingsPanel)
    expect(document.body.querySelector('.modal-mask')).not.toBeNull()
    expect(document.body.querySelector('.settings-sidebar')).not.toBeNull()
    expect(document.body.querySelector('.settings-main-content')).not.toBeNull()
    wrapper.unmount()
  })

  it('add-source dialog renders mask and url input', () => {
    const wrapper = mountShell(AddSourceDialog)
    expect(document.body.querySelector('.modal-mask')).not.toBeNull()
    expect(document.body.querySelector('input')).not.toBeNull()
    wrapper.unmount()
  })

  it('plain modal with title renders mask and default slot', () => {
    const wrapper = mountShell(Modal, {
      props: { title: 'x' },
      slots: { default: '<div class="probe-body">probe</div>' },
    })
    expect(document.body.querySelector('.modal-mask')).not.toBeNull()
    expect(document.body.querySelector('.probe-body')).not.toBeNull()
    wrapper.unmount()
  })

  it('caller v-if=false renders no mask (ConfirmModal contract)', () => {
    const Host = defineComponent({
      components: { Modal },
      template: `<Modal v-if="false" title="x"><div class="probe-body">probe</div></Modal>`,
    })
    const wrapper = mountShell(Host)
    expect(document.body.querySelector('.modal-mask')).toBeNull()
    wrapper.unmount()
  })
})
