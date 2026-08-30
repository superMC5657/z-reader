<script setup lang="ts">
import { useDataStore } from '../../stores/data'
import { useAppStore } from '../../stores/app'
import { formatTime } from '../../lib/time'
import type { Item } from '../../types'

defineProps<{ items: Item[] }>()
const emit = defineEmits<{ select: [Item]; context: [MouseEvent, Item] }>()
const data = useDataStore()
const app = useAppStore()
</script>

<template>
  <div class="compact">
    <article
      v-for="item in items"
      :key="item.id"
      class="row"
      :class="{ faded: app.fadeRead && item.hasBeenRead, selected: data.selectedId === item.id }"
      @click="emit('select', item)"
      @contextmenu.prevent="emit('context', $event, item)"
    >
      <span v-if="!item.hasBeenRead" class="unread-dot"></span>
      <span class="title">{{ item.title }}</span>
      <span class="sep">—</span>
      <span class="snippet">{{ item.snippet }}</span>
      <div class="spacer"></div>
      <span class="source">{{ data.sourceById(item.sourceId)?.title }}</span>
      <span class="time">{{ formatTime(item.publishedAt) }}</span>
      <button class="f-icon-btn star" :class="{ active: item.starred }" @click.stop="data.toggleStar(item)">
        {{ item.starred ? '★' : '☆' }}
      </button>
    </article>
  </div>
</template>

<style scoped>
.compact {
  padding: 0.2rem 1rem;
}

.row {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  padding: 0.32rem 0.5rem;
  border-radius: 4px;
  cursor: pointer;
  font-size: 0.82rem;
}

.row:hover {
  background: var(--bg-hover);
}

.row.selected {
  background: var(--bg-active);
}

.title {
  font-weight: 600;
  flex-shrink: 0;
  max-width: 45%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.sep {
  color: var(--text-tertiary);
  flex-shrink: 0;
}

.snippet {
  color: var(--text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.spacer {
  flex: 1;
}

.source {
  color: var(--accent);
  flex-shrink: 0;
}

.time {
  color: var(--text-tertiary);
  font-size: 0.72rem;
  flex-shrink: 0;
  width: 2.6rem;
  text-align: right;
}

.star {
  width: 1.5rem;
  height: 1.5rem;
}

.star.active {
  color: #f2b705;
}
</style>
