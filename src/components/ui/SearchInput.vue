<script setup lang="ts">
import { ref, watch, onUnmounted } from 'vue'
import Icon from './Icon.vue'

interface Props {
  modelValue?: string
  placeholder?: string
  debounceTime?: number
  disabled?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  modelValue: '',
  placeholder: '',
  debounceTime: 250,
  disabled: false,
})

const emit = defineEmits<{
  'update:modelValue': [value: string]
  search: [value: string]
  input: [value: string]
  clear: []
}>()

const inputRef = ref<HTMLInputElement | null>(null)
const localValue = ref(props.modelValue ?? '')
let debounceTimer: number | undefined

watch(
  () => props.modelValue,
  (newVal) => {
    if ((newVal ?? '') !== localValue.value) {
      localValue.value = newVal ?? ''
    }
  },
)

function triggerSearch(val: string) {
  emit('update:modelValue', val)
  emit('search', val)
}

function handleInput(event: Event) {
  const val = (event.target as HTMLInputElement).value
  localValue.value = val
  emit('input', val)

  if (debounceTimer !== undefined) {
    window.clearTimeout(debounceTimer)
  }

  if (props.debounceTime <= 0) {
    triggerSearch(val)
  } else {
    debounceTimer = window.setTimeout(() => {
      triggerSearch(val)
    }, props.debounceTime)
  }
}

function clear() {
  if (debounceTimer !== undefined) {
    window.clearTimeout(debounceTimer)
  }
  localValue.value = ''
  triggerSearch('')
  emit('clear')
  inputRef.value?.focus()
}

onUnmounted(() => {
  if (debounceTimer !== undefined) {
    window.clearTimeout(debounceTimer)
  }
})

defineExpose({
  inputRef,
  clear,
  focus: () => inputRef.value?.focus(),
  blur: () => inputRef.value?.blur(),
})
</script>

<template>
  <div
    class="search-wrapper"
    :class="{
      'has-search': !!localValue || !!modelValue,
      disabled,
    }"
  >
    <Icon name="search" :size="13" color="var(--text-tertiary)" class="search-icon" />
    <input
      ref="inputRef"
      type="text"
      class="search-input"
      :value="localValue"
      :placeholder="placeholder"
      :disabled="disabled"
      @input="handleInput"
    />
    <button
      v-if="!!localValue || !!modelValue"
      type="button"
      class="clear-btn"
      title="Clear"
      :disabled="disabled"
      @click="clear"
    >
      <Icon name="xmark" :size="14" color="var(--text-tertiary)" />
    </button>
  </div>
</template>

<style scoped>
/* Apple 胶囊搜索框样式 */
.search-wrapper {
  position: relative;
  display: flex;
  align-items: center;
  width: 8rem;
  min-width: 2rem;
  flex-shrink: 1;
  transition: width 0.2s var(--ease);
}

.search-icon {
  position: absolute;
  left: 0.65rem;
  pointer-events: none;
}

.search-input {
  width: 100%;
  font-size: 0.82rem;
  border-radius: var(--radius-pill);
  background: var(--bg-input);
  padding: 0.32rem 1.8rem 0.32rem 1.85rem;
  border: 1px solid var(--border-subtle);
  color: var(--text-primary);
  outline: none;
  transition: all 0.2s var(--ease);
}

.search-input:focus {
  background: var(--bg-input-focus);
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-tint);
}

.search-input::placeholder {
  color: var(--text-tertiary);
}

.clear-btn {
  position: absolute;
  right: 0.5rem;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 2px;
  border-radius: 50%;
  border: none;
  background: transparent;
  cursor: pointer;
  transition: opacity 0.15s ease;
}

.clear-btn:hover {
  opacity: 0.75;
}

.search-wrapper.disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.search-wrapper.disabled .search-input {
  cursor: not-allowed;
}
</style>
