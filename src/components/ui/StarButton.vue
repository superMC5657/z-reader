<script setup lang="ts">
import { computed } from 'vue'
import Icon from './Icon.vue'

interface Props {
  starred?: boolean
  modelValue?: boolean
  size?: number
  disabled?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  starred: false,
  modelValue: undefined,
  size: 15,
  disabled: false,
})

const emit = defineEmits<{
  'update:starred': [value: boolean]
  'update:modelValue': [value: boolean]
  change: [value: boolean]
  click: [event: MouseEvent]
}>()

const isStarred = computed(() => {
  if (props.modelValue !== undefined) return props.modelValue
  return props.starred
})

function handleClick(event: MouseEvent) {
  if (props.disabled) return
  const next = !isStarred.value
  emit('update:starred', next)
  emit('update:modelValue', next)
  emit('change', next)
  emit('click', event)
}
</script>

<template>
  <button
    type="button"
    class="f-icon-btn star-btn"
    :class="{ 'active-star': isStarred }"
    :disabled="disabled"
    @click="handleClick"
  >
    <Icon :name="isStarred ? 'star-fill' : 'star'" :size="size" />
  </button>
</template>

<style scoped>
.star-btn {
  width: 1.8rem;
  height: 1.8rem;
  border-radius: 6px;
  color: var(--text-tertiary);
  transition: all 0.18s var(--ease);
}

.star-btn:hover:not(:disabled) {
  color: var(--star);
  background: var(--bg-hover);
}

.star-btn.active-star {
  color: var(--star) !important;
}
</style>
