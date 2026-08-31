<script setup lang="ts">
import { computed } from 'vue'
import { useUiStore } from '../../stores/data'
import Icon from './Icon.vue'

const ui = useUiStore()

const style = computed(() => ({
  left: `${Math.min(ui.x, window.innerWidth - 240)}px`,
  top: `${Math.min(ui.y, window.innerHeight - ui.items.length * 36 - 20)}px`,
}))

function onAction(action: () => void) {
  ui.closeMenu()
  action()
}
</script>

<template>
  <Teleport to="body">
    <div
      v-if="ui.menuVisible"
      class="ctx-mask"
      @click="ui.closeMenu()"
      @contextmenu.prevent="ui.closeMenu()"
    >
      <div class="ctx-menu" :style="style">
        <button
          v-for="item in ui.items"
          :key="item.label"
          class="ctx-item"
          :class="{ danger: item.danger, active: item.checked }"
          @click="onAction(item.action)"
        >
          <Icon v-if="item.icon" :name="item.icon" :size="14" class="ctx-item-icon" />
          <span class="ctx-label">{{ item.label }}</span>
          <Icon v-if="item.checked" name="checkmark" :size="12" class="ctx-check" />
        </button>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.ctx-mask {
  position: fixed;
  inset: 0;
  z-index: 2000;
}

.ctx-menu {
  position: fixed;
  min-width: 14rem;
  background: var(--bg-card);
  backdrop-filter: var(--glass-blur);
  -webkit-backdrop-filter: var(--glass-blur);
  border-radius: 10px;
  box-shadow: var(--shadow-pop);
  border: 0.5px solid var(--border);
  padding: 0.35rem;
  animation: appleMenuIn 0.15s var(--ease);
}

.ctx-item {
  display: flex;
  align-items: center;
  gap: 0.55rem;
  width: 100%;
  text-align: left;
  padding: 0.38rem 0.65rem;
  border-radius: 6px;
  font-size: 0.85rem;
  font-weight: 450;
  color: var(--text-primary);
  transition: all 0.12s var(--ease);
}

.ctx-label {
  flex: 1;
}

.ctx-item-icon {
  flex-shrink: 0;
  color: var(--text-secondary);
}

.ctx-item:hover .ctx-item-icon {
  color: #ffffff;
}

.ctx-check {
  margin-left: auto;
  flex-shrink: 0;
  color: var(--accent);
}

.ctx-item:hover .ctx-check {
  color: #ffffff;
}

.ctx-item:hover {
  background: var(--accent);
  color: #ffffff;
}

.ctx-item.danger {
  color: var(--danger);
}

.ctx-item.danger:hover {
  background: var(--danger);
  color: #ffffff;
}

@keyframes appleMenuIn {
  0% {
    opacity: 0;
    transform: scale(0.96) translateY(-4px);
  }
  100% {
    opacity: 1;
    transform: scale(1) translateY(0);
  }
}
</style>
