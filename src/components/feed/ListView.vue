<script setup lang="ts">
import { useDataStore } from '../../stores/data'
import { formatTime } from '../../lib/time'
import type { Item } from '../../types'

defineProps<{ items: Item[] }>()
const emit = defineEmits<{ select: [Item]; context: [MouseEvent, Item] }>()
const data = useDataStore()
</script>

<template>
  <div class="list">
    <article
      v-for="item in items"
      :key="item.id"
      class="row"
      :class="{ faded: item.hasBeenRead, selected: data.selectedId === item.id }"
      @click="emit('select', item)"
      @contextmenu.prevent="emit('context', $event, item)"
    >
      <span v-if="!item.hasBeenRead" class="unread-dot"></span>
      <div class="row-main">
        <h3 class="title">{{ item.title }}</h3>
        <p v-if="item.snippet" class="snippet">{{ item.snippet }}</p>
      </div>
      <div class="meta">
        <span class="source">{{ data.sourceById(item.sourceId)?.title }}</span>
        <span class="time">{{ formatTime(item.publishedAt) }}</span>
      </div>
      <button class="f-icon-btn star" :class="{ active: item.starred }" @click.stop="data.toggleStar(item)">
        {{ item.starred ? '★' : '☆' }}
      </button>
    </article>
  </div>
</template>

<style scoped>
.list {
  padding: 0.4rem 1rem;
}

.row {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  padding: 0.55rem 0.7rem;
  border-radius: var(--radius);
  cursor: pointer;
}

.row:hover {
  background: var(--bg-hover);
}

.row.selected {
  background: var(--bg-active);
}

.row .unread-dot {
  margin-right: -0.3rem;
}

.row-main {
  flex: 1;
  min-width: 0;
}

.title {
  font-size: 0.92rem;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.snippet {
  font-size: 0.78rem;
  color: var(--text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  margin-top: 0.15rem;
}

.meta {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 0.1rem;
  font-size: 0.72rem;
  color: var(--text-tertiary);
  flex-shrink: 0;
}

.source {
  color: var(--accent);
  max-width: 10rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.star {
  width: 1.6rem;
  height: 1.6rem;
}

.star.active {
  color: #f2b705;
}
</style>
