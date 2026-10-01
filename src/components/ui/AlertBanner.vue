<script setup lang="ts">
import { computed } from 'vue'
import Icon from './Icon.vue'

interface AlertBannerProps {
  type?: 'error' | 'success' | 'info'
  message?: string
  retryable?: boolean
  loading?: boolean
  retryText?: string
  icon?: string
  iconColor?: string
  iconSize?: number | string
}

const props = withDefaults(defineProps<AlertBannerProps>(), {
  type: 'error',
  message: '',
  retryable: false,
  loading: false,
  retryText: '',
  icon: undefined,
  iconColor: undefined,
  iconSize: 14,
})

const emit = defineEmits<{
  retry: []
}>()

const computedIcon = computed(() => {
  if (props.icon) return props.icon
  if (props.type === 'success') return 'checkmark'
  return 'info'
})

const computedIconColor = computed(() => {
  if (props.iconColor) return props.iconColor
  if (props.type === 'success') return 'var(--success)'
  if (props.type === 'error') return 'var(--danger)'
  return 'var(--accent)'
})
</script>

<template>
  <div
    class="alert-banner"
    :class="`alert-banner--${type}`"
    role="alert"
  >
    <slot name="icon">
      <Icon
        :name="computedIcon"
        :size="iconSize"
        :color="computedIconColor"
        class="alert-banner-icon"
      />
    </slot>

    <span class="alert-banner-message">
      <slot>{{ message }}</slot>
    </span>

    <button
      v-if="retryable"
      type="button"
      class="alert-banner-retry-btn"
      :disabled="loading"
      @click="emit('retry')"
    >
      <Icon name="arrow-clockwise" :size="12" :class="{ spin: loading }" />
      <slot name="retry">
        <span v-if="retryText">{{ retryText }}</span>
      </slot>
    </button>
  </div>
</template>

<style scoped>
.alert-banner {
  padding: 0.65rem 0.9rem;
  border-radius: 8px;
  font-size: 0.82rem;
  display: flex;
  align-items: center;
  gap: 0.5rem;
  box-sizing: border-box;
}

.alert-banner--error {
  background: var(--danger-tint);
  color: var(--danger);
}

.alert-banner--success {
  background: var(--bg-card);
  border: 0.5px solid var(--border);
  color: var(--text-secondary);
}

.alert-banner--info {
  background: var(--accent-tint);
  color: var(--accent);
}

.alert-banner-icon {
  flex-shrink: 0;
}

.alert-banner-message {
  flex: 1;
  min-width: 0;
  word-break: break-word;
}

.alert-banner-retry-btn {
  display: inline-flex;
  align-items: center;
  gap: 0.3rem;
  background: transparent;
  border: 1px solid currentColor;
  color: inherit;
  font-size: 0.78rem;
  padding: 0.2rem 0.6rem;
  border-radius: 4px;
  cursor: pointer;
  transition: opacity 0.15s ease;
  white-space: nowrap;
  flex-shrink: 0;
}

.alert-banner-retry-btn:hover:not(:disabled) {
  opacity: 0.75;
}

.alert-banner-retry-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
