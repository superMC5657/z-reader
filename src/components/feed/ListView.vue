<script setup lang="ts">
import { useDataStore } from '../../stores/data'
import { formatTime } from '../../lib/time'
import { highlightText } from '../../lib/highlight'
import type { Item } from '../../types'
import Icon from '../ui/Icon.vue'

defineProps<{ items: Item[] }>()
const emit = defineEmits<{ select: [Item]; context: [MouseEvent, Item] }>()
const data = useDataStore()
</script>

<template>
  <div class="list-container">
    <article
      v-for="item in items"
      :key="item.id"
      class="row"
      :class="{
        faded: item.hasBeenRead,
        selected: data.selectedId === item.id,
      }"
      @click="emit('select', item)"
      @contextmenu.prevent="emit('context', $event, item)"
    >
      <!-- Unread Indicator -->
      <div class="unread-slot">
        <span v-if="!item.hasBeenRead" class="unread-dot"></span>
      </div>

      <!-- Main Content -->
      <div class="row-main">
        <h3 class="title" v-html="highlightText(item.title, data.search)"></h3>
        <p v-if="item.snippet" class="snippet" v-html="highlightText(item.snippet, data.search)"></p>
      </div>

      <!-- Metadata & Channel -->
      <div class="meta">
        <span class="source">{{ data.sourceById(item.sourceId)?.title }}</span>
        <span class="time">{{ formatTime(item.publishedAt) }}</span>
      </div>

      <!-- Star Button -->
      <button
        class="f-icon-btn star-btn"
        :class="{ 'active-star': item.starred }"
        @click.stop="data.toggleStar(item)"
      >
        <Icon :name="item.starred ? 'star-fill' : 'star'" :size="14" />
      </button>
    </article>
  </div>
</template>

<style scoped>
.list-container {
  padding: 0.6rem 1rem;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.row {
  display: flex;
  align-items: center;
  gap: 0.65rem;
  padding: 0.6rem 0.8rem;
  border-radius: 9px;
  cursor: pointer;
  transition: all 0.16s var(--ease);
  position: relative;
}

.row:hover:not(.selected) {
  background: var(--bg-hover);
}

.row.selected {
  background: var(--accent);
  color: #ffffff;
  box-shadow: 0 1px 3px rgba(0, 122, 255, 0.3);
}

.unread-slot {
  width: 10px;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.row.selected .unread-dot {
  background: #ffffff;
  box-shadow: 0 0 6px rgba(255, 255, 255, 0.8);
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
  letter-spacing: -0.015em;
  color: var(--text-primary);
  line-height: 1.35;
}

.row.selected .title {
  color: #ffffff;
}

.snippet {
  font-size: 0.78rem;
  color: var(--text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  margin-top: 0.2rem;
  letter-spacing: -0.01em;
}

.row.selected .snippet {
  color: rgba(255, 255, 255, 0.82);
}

.meta {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 0.15rem;
  font-size: 0.72rem;
  color: var(--text-tertiary);
  flex-shrink: 0;
  font-variant-numeric: tabular-nums;
  min-width: 5.5rem;
}

.source {
  color: var(--accent);
  font-weight: 600;
  max-width: 9rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.row.selected .source {
  color: #ffffff;
}

.time {
  font-size: 0.7rem;
}

.row.selected .time {
  color: rgba(255, 255, 255, 0.75);
}

.star-btn {
  width: 1.7rem;
  height: 1.7rem;
  border-radius: 6px;
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
