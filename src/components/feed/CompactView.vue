<script setup lang="ts">
import { useDataStore } from '../../stores/data'
import { useAppStore } from '../../stores/app'
import { formatTime } from '../../lib/time'
import type { Item } from '../../types'
import Icon from '../ui/Icon.vue'

defineProps<{ items: Item[] }>()
const emit = defineEmits<{ select: [Item]; context: [MouseEvent, Item] }>()
const data = useDataStore()
const app = useAppStore()
</script>

<template>
  <div class="compact-container">
    <article
      v-for="item in items"
      :key="item.id"
      class="row"
      :class="{
        faded: app.fadeRead && item.hasBeenRead,
        selected: data.selectedId === item.id,
      }"
      @click="emit('select', item)"
      @contextmenu.prevent="emit('context', $event, item)"
    >
      <div class="unread-slot">
        <span v-if="!item.hasBeenRead" class="unread-dot"></span>
      </div>

      <span class="title">{{ item.title }}</span>

      <span class="sep">—</span>

      <span class="snippet">{{ item.snippet }}</span>

      <div class="spacer"></div>

      <span class="source">{{ data.sourceById(item.sourceId)?.title }}</span>

      <span class="time">{{ formatTime(item.publishedAt) }}</span>

      <button
        class="f-icon-btn star-btn"
        :class="{ 'active-star': item.starred }"
        @click.stop="data.toggleStar(item)"
      >
        <Icon :name="item.starred ? 'star-fill' : 'star'" :size="13" />
      </button>
    </article>
  </div>
</template>

<style scoped>
.compact-container {
  padding: 0.4rem 0.9rem;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.row {
  display: flex;
  align-items: center;
  gap: 0.45rem;
  padding: 0.38rem 0.65rem;
  border-radius: 7px;
  cursor: pointer;
  font-size: 0.84rem;
  transition: all 0.15s var(--ease);
}

.row:hover:not(.selected) {
  background: var(--bg-hover);
}

.row.selected {
  background: var(--accent);
  color: #ffffff;
}

.unread-slot {
  width: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.row.selected .unread-dot {
  background: #ffffff;
}

.title {
  font-weight: 550;
  flex-shrink: 0;
  max-width: 44%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  letter-spacing: -0.015em;
  color: var(--text-primary);
}

.row.selected .title {
  color: #ffffff;
}

.sep {
  color: var(--text-quaternary);
  flex-shrink: 0;
}

.row.selected .sep {
  color: rgba(255, 255, 255, 0.6);
}

.snippet {
  color: var(--text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 0.8rem;
  letter-spacing: -0.01em;
}

.row.selected .snippet {
  color: rgba(255, 255, 255, 0.85);
}

.spacer {
  flex: 1;
}

.source {
  color: var(--accent);
  font-weight: 600;
  font-size: 0.78rem;
  flex-shrink: 0;
  max-width: 8.5rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.row.selected .source {
  color: #ffffff;
}

.time {
  color: var(--text-tertiary);
  font-size: 0.72rem;
  flex-shrink: 0;
  width: 4.2rem;
  text-align: right;
  white-space: nowrap;
  font-variant-numeric: tabular-nums;
}

.row.selected .time {
  color: rgba(255, 255, 255, 0.8);
}

.star-btn {
  width: 1.6rem;
  height: 1.6rem;
  border-radius: 5px;
  color: var(--text-tertiary);
}

.row.selected .star-btn {
  color: rgba(255, 255, 255, 0.8);
}

.row.selected .star-btn:hover {
  background: rgba(255, 255, 255, 0.2);
}

.star-btn.active-star {
  color: var(--star) !important;
}

.row.selected .star-btn.active-star {
  color: #ffd60a !important;
}
</style>
