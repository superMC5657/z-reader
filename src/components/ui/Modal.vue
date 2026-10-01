<script setup lang="ts">
import { computed } from 'vue'
import Icon from './Icon.vue'

const props = withDefaults(
  defineProps<{
    title?: string
    wide?: boolean
    extraWide?: boolean
    customLayout?: boolean
    preset?: 'default' | 'sheet' | 'acrylic'
    show?: boolean
    transition?: string
  }>(),
  {
    preset: 'default',
  }
)

const emit = defineEmits<{ close: [] }>()

const isCustomLayout = computed(() => {
  if (props.customLayout !== undefined) {
    return props.customLayout
  }
  if (props.preset === 'acrylic' && !props.title) {
    return true
  }
  return false
})

const transitionName = computed(() => {
  if (props.transition) {
    return props.transition
  }
  if (props.preset === 'acrylic') {
    return 'focus-sheet'
  }
  if (props.preset === 'sheet') {
    return 'modal-sheet'
  }
  return 'modal-default'
})
</script>

<template>
  <Teleport to="body">
    <Transition :name="transitionName" appear>
      <div
        v-if="show ?? true"
        class="modal-mask"
        :class="[
          `preset-${preset}`,
          {
            'focus-backdrop': preset === 'acrylic',
            acrylic: preset === 'acrylic',
            sheet: preset === 'sheet',
          },
        ]"
        @click.self="emit('close')"
      >
        <slot name="left" />
        <slot name="prefix" />

        <div
          class="modal-card"
          :class="[
            `preset-${preset}`,
            {
              wide,
              'extra-wide': extraWide,
              'custom-modal': isCustomLayout,
              'focus-modal-card': preset === 'acrylic',
              acrylic: preset === 'acrylic',
              sheet: preset === 'sheet',
            },
          ]"
        >
          <template v-if="!isCustomLayout">
            <div class="modal-header">
              <span class="header-title">{{ title }}</span>
              <button class="f-icon-btn close-btn" title="Close" @click="emit('close')">
                <Icon name="xmark" :size="16" />
              </button>
            </div>
            <div class="modal-body">
              <slot />
            </div>
            <div v-if="$slots.footer" class="modal-footer">
              <slot name="footer" />
            </div>
          </template>
          <template v-else>
            <slot />
          </template>
        </div>

        <slot name="suffix" />
        <slot name="right" />
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.header-title {
  font-size: 1.15rem;
  font-weight: 700;
  letter-spacing: -0.02em;
  color: var(--text-primary);
}

.close-btn {
  width: 1.7rem;
  height: 1.7rem;
  border-radius: 50%;
  color: var(--text-tertiary);
}

.close-btn:hover {
  background: var(--bg-hover-strong);
  color: var(--text-primary);
}

/* Acrylic 预设样式 (对齐 App.vue 专注模式) */
.modal-mask.acrylic {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.45);
  backdrop-filter: blur(24px) saturate(180%);
  -webkit-backdrop-filter: blur(24px) saturate(180%);
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 1.25rem;
  z-index: 1200;
  padding: 1.25rem 1.5rem;
  animation: none;
}

:global([data-theme='dark']) .modal-mask.acrylic {
  background: rgba(0, 0, 0, 0.62);
}

@media (max-width: 1040px) {
  .modal-mask.acrylic {
    padding: 1rem 0.5rem;
    gap: 0.5rem;
  }
}

.modal-card.acrylic {
  width: 780px;
  min-width: 0;
  max-width: 94vw;
  height: 90vh;
  max-height: 92vh;
  background: var(--bg);
  border-radius: 16px;
  box-shadow: 0 24px 64px rgba(0, 0, 0, 0.4), 0 0 0 1px var(--border);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  animation: none;
}

/* Acrylic / focus-sheet 过渡动效 */
.focus-sheet-enter-active,
.focus-sheet-leave-active {
  transition: opacity 0.22s cubic-bezier(0.16, 1, 0.3, 1);
}

.focus-sheet-enter-active .focus-modal-card,
.focus-sheet-leave-active .focus-modal-card,
.focus-sheet-enter-active .modal-card.acrylic,
.focus-sheet-leave-active .modal-card.acrylic {
  transition: transform 0.26s cubic-bezier(0.16, 1, 0.3, 1), opacity 0.22s ease;
}

.focus-sheet-enter-from,
.focus-sheet-leave-to {
  opacity: 0;
}

.focus-sheet-enter-from .focus-modal-card,
.focus-sheet-leave-to .focus-modal-card,
.focus-sheet-enter-from .modal-card.acrylic,
.focus-sheet-leave-to .modal-card.acrylic {
  opacity: 0;
  transform: scale(0.95) translateY(12px);
}

/* Sheet 预设过渡动效 */
.modal-sheet-enter-active,
.modal-sheet-leave-active {
  transition: opacity 0.2s cubic-bezier(0.16, 1, 0.3, 1);
}

.modal-sheet-enter-active .modal-card,
.modal-sheet-leave-active .modal-card {
  transition: transform 0.26s cubic-bezier(0.16, 1, 0.3, 1), opacity 0.2s ease;
}

.modal-sheet-enter-from,
.modal-sheet-leave-to {
  opacity: 0;
}

.modal-sheet-enter-from .modal-card,
.modal-sheet-leave-to .modal-card {
  opacity: 0;
  transform: scale(0.96) translateY(8px);
}
</style>
