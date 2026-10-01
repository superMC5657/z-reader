<script setup lang="ts">
import Icon from './Icon.vue'

interface EmptyStateProps {
  icon?: string
  iconSize?: number | string
  title?: string
  description?: string
  actionText?: string
  actionLoading?: boolean
}

withDefaults(defineProps<EmptyStateProps>(), {
  icon: '',
  iconSize: 32,
  title: '',
  description: '',
  actionText: '',
  actionLoading: false,
})

const emit = defineEmits<{
  action: []
}>()
</script>

<template>
  <div class="empty-state">
    <slot name="icon">
      <div v-if="icon" class="empty-icon-circle">
        <Icon :name="icon" :size="iconSize" color="var(--text-quaternary)" />
      </div>
    </slot>

    <slot name="title">
      <p v-if="title" class="empty-title">{{ title }}</p>
    </slot>

    <slot name="description">
      <p v-if="description" class="empty-desc">{{ description }}</p>
    </slot>

    <slot name="action">
      <button
        v-if="actionText"
        type="button"
        class="f-btn primary empty-action-btn"
        :disabled="actionLoading"
        @click="emit('action')"
      >
        <Icon
          v-if="actionLoading"
          name="arrow-clockwise"
          :size="14"
          class="spin"
        />
        <span>{{ actionText }}</span>
      </button>
    </slot>

    <slot />
  </div>
</template>

<style scoped>
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 100%;
  width: 100%;
  padding: 2rem;
  gap: 0.55rem;
  color: var(--text-tertiary);
  text-align: center;
  box-sizing: border-box;
}

.empty-icon-circle {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 4rem;
  height: 4rem;
  border-radius: 50%;
  background: var(--bg-track);
  margin-bottom: 0.35rem;
  flex-shrink: 0;
}

.empty-title {
  font-size: 1rem;
  font-weight: 600;
  color: var(--text-secondary);
  line-height: 1.4;
  margin: 0;
}

.empty-desc {
  font-size: 0.82rem;
  color: var(--text-tertiary);
  line-height: 1.4;
  margin: 0;
}

.empty-action-btn {
  margin-top: 0.25rem;
}
</style>
