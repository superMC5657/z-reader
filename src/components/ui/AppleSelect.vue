<script setup lang="ts">
import { computed } from 'vue'
import { useUiStore } from '../../stores/data'
import Icon from './Icon.vue'

export interface SelectOption<T = any> {
  value: T
  label: string
  icon?: string
}

const props = withDefaults(
  defineProps<{
    modelValue: any
    options: SelectOption[]
    placeholder?: string
    disabled?: boolean
    compact?: boolean
  }>(),
  {
    placeholder: '',
    disabled: false,
    compact: false,
  }
)

const emit = defineEmits<{
  'update:modelValue': [value: any]
  change: [value: any]
}>()

const ui = useUiStore()

const currentOption = computed(() => {
  return props.options.find((opt) => opt.value === props.modelValue)
})

function openMenu(e: MouseEvent) {
  if (props.disabled) return
  const target = (e.currentTarget || e.target) as HTMLElement
  const rect = target.getBoundingClientRect()

  const menuItems = props.options.map((opt) => ({
    label: opt.label,
    icon: opt.icon,
    checked: opt.value === props.modelValue,
    action: () => {
      emit('update:modelValue', opt.value)
      emit('change', opt.value)
    },
  }))

  // Anchor menu: right-align if trigger is on the right side of the screen/container
  const estimatedMenuWidth = Math.max(rect.width, props.compact ? 120 : 150)
  const isRightSide = rect.left > window.innerWidth / 2 || rect.right + 20 > window.innerWidth
  const posX = isRightSide
    ? Math.max(10, rect.right - estimatedMenuWidth)
    : Math.max(10, rect.left)

  ui.openMenu(posX, rect.bottom + 4, menuItems)
}
</script>

<template>
  <button
    type="button"
    class="apple-select-btn"
    :class="{ compact, disabled }"
    :disabled="disabled"
    @click="openMenu"
  >
    <div class="selected-content">
      <Icon
        v-if="currentOption?.icon"
        :name="currentOption.icon"
        :size="compact ? 12 : 13.5"
        class="selected-icon"
      />
      <span class="selected-label">
        {{ currentOption?.label ?? placeholder }}
      </span>
    </div>
    <Icon
      name="chevrons-up-down"
      :size="10"
      class="select-chevron"
      color="var(--text-tertiary)"
    />
  </button>
</template>

<style scoped>
.apple-select-btn {
  display: inline-flex;
  align-items: center;
  gap: 0.45rem;
  padding: 0.32rem 0.65rem;
  background: var(--bg-input);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-input);
  color: var(--text-primary);
  font: inherit;
  font-size: 0.82rem;
  font-weight: 500;
  cursor: pointer;
  outline: none;
  min-width: 6.8rem;
  max-width: 10.5rem;
  justify-content: space-between;
  transition: all 0.18s var(--ease);
  user-select: none;
  box-sizing: border-box;
}

.apple-select-btn:hover:not(:disabled) {
  background: var(--bg-hover-strong);
  border-color: var(--border);
}

.apple-select-btn:active:not(:disabled),
.apple-select-btn:focus-visible {
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-tint);
}

.apple-select-btn.compact {
  padding: 0.2rem 0.48rem;
  font-size: 0.77rem;
  min-width: 5.2rem;
  max-width: 8rem;
  border-radius: 6px;
}

.apple-select-btn.disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.selected-content {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  min-width: 0;
  flex: 1;
}

.selected-icon {
  flex-shrink: 0;
  color: var(--accent);
}

.selected-label {
  flex: 1;
  text-align: left;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.select-chevron {
  flex-shrink: 0;
  margin-left: 0.2rem;
  opacity: 0.7;
}

.apple-select-btn:hover .select-chevron {
  opacity: 1;
}
</style>
