<script setup lang="ts">
export interface StepperSliderProps {
  modelValue: number
  min?: number
  max?: number
  step?: number
  unit?: string
  disabled?: boolean
  decreaseLabel?: string
  increaseLabel?: string
  decreaseTitle?: string
  increaseTitle?: string
  resetValue?: number
  badgeTitle?: string
}

const props = withDefaults(defineProps<StepperSliderProps>(), {
  min: 0,
  max: 100,
  step: 1,
  unit: '',
  disabled: false,
  decreaseLabel: '-',
  increaseLabel: '+',
  decreaseTitle: undefined,
  increaseTitle: undefined,
  resetValue: undefined,
  badgeTitle: undefined,
})

const emit = defineEmits<{
  'update:modelValue': [value: number]
  change: [value: number]
  reset: [value: number]
}>()

function clamp(val: number, min: number, max: number): number {
  return Math.min(Math.max(val, min), max)
}

function roundToStep(val: number, step: number): number {
  const stepStr = step.toString()
  const precision = stepStr.includes('.') ? stepStr.split('.')[1].length : 0
  const factor = 10 ** precision
  return Math.round(val * factor) / factor
}

function adjust(delta: number) {
  if (props.disabled) return
  const current = Number(props.modelValue ?? props.min)
  const next = clamp(roundToStep(current + delta, props.step), props.min, props.max)
  if (next !== current) {
    emit('update:modelValue', next)
    emit('change', next)
  }
}

function onInput(event: Event) {
  const target = event.target as HTMLInputElement
  const next = Number(target.value)
  emit('update:modelValue', next)
  emit('change', next)
}

function onBadgeClick() {
  if (props.disabled) return
  if (props.resetValue !== undefined) {
    emit('update:modelValue', props.resetValue)
    emit('change', props.resetValue)
    emit('reset', props.resetValue)
  }
}
</script>

<template>
  <div class="slider-row" :class="{ disabled }">
    <button
      type="button"
      class="f-icon-btn stepper-btn"
      :title="decreaseTitle || decreaseLabel"
      :disabled="disabled || modelValue <= min"
      @click="adjust(-step)"
    >
      <slot name="decrease">
        <span class="stepper-label small">{{ decreaseLabel }}</span>
      </slot>
    </button>
    <input
      type="range"
      :min="min"
      :max="max"
      :step="step"
      :value="modelValue"
      :disabled="disabled"
      @input="onInput"
    />
    <button
      type="button"
      class="f-icon-btn stepper-btn"
      :title="increaseTitle || increaseLabel"
      :disabled="disabled || modelValue >= max"
      @click="adjust(step)"
    >
      <slot name="increase">
        <span class="stepper-label large">{{ increaseLabel }}</span>
      </slot>
    </button>
    <span
      class="value-badge"
      :class="{ clickable: resetValue !== undefined }"
      :style="resetValue !== undefined ? { cursor: 'pointer' } : undefined"
      :title="badgeTitle"
      @click="onBadgeClick"
    >
      <slot name="badge" :value="modelValue" :unit="unit">
        {{ modelValue }}{{ unit }}
      </slot>
    </span>
  </div>
</template>

<style scoped>
.slider-row {
  display: flex;
  align-items: center;
  gap: 0.45rem;
}

.stepper-btn {
  width: 1.65rem;
  height: 1.65rem;
  border-radius: 6px;
  background: var(--bg-track);
  color: var(--text-secondary);
}

.stepper-btn:hover:not(:disabled) {
  background: var(--bg-hover-strong);
  color: var(--text-primary);
}

.stepper-label {
  font-weight: 600;
  line-height: 1;
  user-select: none;
}

.stepper-label.small {
  font-size: 0.7rem;
}

.stepper-label.large {
  font-size: 0.82rem;
}

.value-badge {
  font-size: 0.78rem;
  font-weight: 600;
  color: var(--text-secondary);
  background: var(--bg-track);
  padding: 0.1rem 0.45rem;
  border-radius: var(--radius-pill);
  font-variant-numeric: tabular-nums;
  user-select: none;
}

.value-badge.clickable {
  cursor: pointer;
}

.slider-row.disabled {
  opacity: 0.45;
  pointer-events: none;
}
</style>
