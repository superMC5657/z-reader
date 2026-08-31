<script setup lang="ts">
import { computed } from 'vue'
import { useUiStore } from '../../stores/data'

const ui = useUiStore()

const style = computed(() => ({
  left: `${Math.min(ui.x, window.innerWidth - 230)}px`,
  top: `${Math.min(ui.y, window.innerHeight - ui.items.length * 38 - 16)}px`,
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
          :class="{ danger: item.danger }"
          @click="onAction(item.action)"
        >
          {{ item.label }}
        </button>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.ctx-mask {
  position: fixed;
  inset: 0;
  z-index: 200;
}

.ctx-menu {
  position: fixed;
  min-width: 13rem;
  background: var(--bg-card);
  backdrop-filter: blur(28px) saturate(1.6);
  border-radius: 8px;
  box-shadow: var(--shadow-pop), 0 0 0 0.5px var(--border);
  padding: 0.3rem;
}

.ctx-item {
  display: block;
  width: 100%;
  text-align: left;
  padding: 0.32rem 0.75rem;
  border-radius: 5px;
  font-size: 0.88rem;
  transition: background 0.1s var(--ease);
}

.ctx-item:hover {
  background: var(--accent);
  color: #fff;
}

.ctx-item.danger {
  color: var(--danger);
}

.ctx-item.danger:hover {
  background: var(--danger);
  color: #fff;
}
</style>
