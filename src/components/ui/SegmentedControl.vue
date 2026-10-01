<script setup lang="ts">
import Icon from './Icon.vue'

export interface SegmentedOption<T = any> {
  label?: string
  value: T
  icon?: string
  title?: string
  disabled?: boolean
}

interface Props {
  modelValue: any
  options: SegmentedOption[]
  size?: 'normal' | 'compact'
  disabled?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  size: 'normal',
  disabled: false,
})

const emit = defineEmits<{
  'update:modelValue': [value: any]
  change: [value: any]
}>()

function selectOption(opt: SegmentedOption) {
  if (props.disabled || opt.disabled) return
  if (props.modelValue !== opt.value) {
    emit('update:modelValue', opt.value)
    emit('change', opt.value)
  }
}
</script>

<template>
  <div class="segmented" :class="[size, { disabled }]">
    <button
      v-for="opt in options"
      :key="String(opt.value)"
      type="button"
      class="seg"
      :class="{
        active: modelValue === opt.value,
        'icon-only': !opt.label && !!opt.icon,
      }"
      :title="opt.title || opt.label"
      :disabled="disabled || opt.disabled"
      @click="selectOption(opt)"
    >
      <slot name="option" :option="opt" :active="modelValue === opt.value">
        <Icon v-if="opt.icon" :name="opt.icon" :size="size === 'compact' ? 13 : 14" />
        <span v-if="opt.label">{{ opt.label }}</span>
      </slot>
    </button>
  </div>
</template>

<style scoped>
.segmented.disabled {
  opacity: 0.5;
  pointer-events: none;
}

.segmented.compact {
  border-radius: 7px;
  padding: 2px;
  gap: 1.5px;
}

.segmented.compact .seg {
  padding: 0.2rem 0.5rem;
  font-size: 0.78rem;
  min-height: 1.35rem;
  border-radius: 5px;
}

.seg.icon-only {
  padding: 0.25rem 0.55rem;
}

.segmented.compact .seg.icon-only {
  padding: 0.2rem 0.45rem;
}

.seg:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
</style>
