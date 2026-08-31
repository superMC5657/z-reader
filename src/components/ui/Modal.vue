<script setup lang="ts">
import Icon from './Icon.vue'

defineProps<{ title: string; wide?: boolean }>()
const emit = defineEmits<{ close: [] }>()
</script>

<template>
  <Teleport to="body">
    <div class="modal-mask" @click.self="emit('close')">
      <div class="modal-card" :class="{ wide }">
        <div class="modal-header">
          <span class="header-title">{{ title }}</span>
          <button class="f-icon-btn close-btn" title="Close" @click="emit('close')">
            <Icon name="xmark" :size="16" />
          </button>
        </div>
        <div class="modal-body">
          <slot />
        </div>
        <div class="modal-footer">
          <slot name="footer" />
        </div>
      </div>
    </div>
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
</style>
