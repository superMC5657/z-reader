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
  min-width: 12rem;
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: 8px;
  box-shadow: var(--shadow-flyout);
  padding: 0.25rem;
}

.ctx-item {
  display: block;
  width: 100%;
  text-align: left;
  padding: 0.4rem 0.7rem;
  border-radius: 4px;
}

.ctx-item:hover {
  background: var(--bg-hover);
}

.ctx-item.danger {
  color: var(--danger);
}
</style>
