<script setup lang="ts">
defineProps<{
  modelValue: boolean
  disabled?: boolean
}>()

const emit = defineEmits<{
  'update:modelValue': [value: boolean]
}>()

function toggle(e: Event) {
  const target = e.target as HTMLInputElement
  emit('update:modelValue', target.checked)
}
</script>

<template>
  <label class="apple-switch" :class="{ disabled }">
    <input
      type="checkbox"
      :checked="modelValue"
      :disabled="disabled"
      @change="toggle"
    />
    <span class="slider"></span>
  </label>
</template>

<style scoped>
.apple-switch {
  position: relative;
  display: inline-block;
  width: 44px;
  height: 26px;
  flex-shrink: 0;
  cursor: pointer;
  user-select: none;
}

.apple-switch.disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.apple-switch input {
  opacity: 0;
  width: 0;
  height: 0;
  position: absolute;
}

.slider {
  position: absolute;
  inset: 0;
  background-color: var(--apple-switch-bg, rgba(120, 120, 128, 0.22));
  border-radius: 999px;
  transition: background-color 0.25s cubic-bezier(0.16, 1, 0.3, 1), box-shadow 0.2s ease;
  box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.04);
}

.slider::before {
  content: '';
  position: absolute;
  height: 22px;
  width: 22px;
  left: 2px;
  top: 2px;
  background-color: #ffffff;
  border-radius: 50%;
  transition: transform 0.25s cubic-bezier(0.16, 1, 0.3, 1), width 0.15s ease;
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.18), 0 1px 1px rgba(0, 0, 0, 0.1);
}

.apple-switch input:checked + .slider {
  background-color: var(--accent);
}

.apple-switch input:checked + .slider::before {
  transform: translateX(18px);
}

.apple-switch:active:not(.disabled) .slider::before {
  width: 24px;
}

.apple-switch input:focus-visible + .slider {
  box-shadow: 0 0 0 3px var(--accent-tint), inset 0 0 0 1px var(--accent);
}
</style>
