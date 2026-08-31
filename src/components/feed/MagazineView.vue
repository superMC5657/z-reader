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
  <div class="magazine">
    <article
      v-for="item in items"
      :key="item.id"
      class="row"
      :class="{ faded: app.fadeRead && item.hasBeenRead, selected: data.selectedId === item.id }"
      @click="emit('select', item)"
      @contextmenu.prevent="emit('context', $event, item)"
    >
      <div v-if="app.showCover && item.image" class="thumb">
        <img :src="item.image" loading="lazy" alt="" />
      </div>
      <div class="row-main">
        <div class="meta">
          <span class="source">{{ data.sourceById(item.sourceId)?.title }}</span>
          <span class="dot">·</span>
          <span>{{ formatTime(item.publishedAt) }}</span>
          <div class="spacer"></div>
          <span v-if="!item.hasBeenRead" class="unread-dot"></span>
        </div>
        <h3 class="title">{{ item.title }}</h3>
        <p v-if="app.showSnippet && item.snippet" class="snippet">{{ item.snippet }}</p>
        <div class="foot">
          <span class="author">{{ item.author }}</span>
          <div class="spacer"></div>
          <button class="f-icon-btn star" :class="{ active: item.starred }" @click.stop="data.toggleStar(item)">
            {{ item.starred ? '★' : '☆' }}
          </button>
        </div>
      </div>
    </article>
  </div>
</template>

<style scoped>
.magazine {
  padding: 0.6rem 1.2rem;
  display: flex;
  flex-direction: column;
  gap: 0.6rem;
}

.row {
  display: flex;
  gap: 0.9rem;
  background: var(--bg-card);
  border-radius: var(--radius-card);
  box-shadow: var(--shadow-card);
  padding: 0.75rem 1rem;
  cursor: pointer;
  transition: transform 0.2s var(--ease), box-shadow 0.2s var(--ease);
}

.row:hover {
  transform: translateY(-1px);
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.1), var(--shadow-card);
}

.row.selected {
  outline: 2.5px solid var(--accent);
  outline-offset: -2.5px;
}

.thumb {
  width: 9.5rem;
  aspect-ratio: 16 / 10;
  border-radius: 8px;
  overflow: hidden;
  flex-shrink: 0;
  background: var(--bg-track);
}

.thumb img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}

.row-main {
  flex: 1;
  min-width: 0;
}

.meta {
  display: flex;
  align-items: center;
  gap: 0.3rem;
  font-size: 0.72rem;
  font-weight: 500;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.02em;
}

.source {
  color: var(--accent);
}

.spacer {
  flex: 1;
}

.title {
  font-size: 1rem;
  font-weight: 600;
  margin: 0.25rem 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.snippet {
  font-size: 0.8rem;
  color: var(--text-secondary);
  line-height: 1.5;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.foot {
  display: flex;
  align-items: center;
  font-size: 0.74rem;
  color: var(--text-tertiary);
  margin-top: 0.35rem;
}

.star {
  width: 1.6rem;
  height: 1.6rem;
}

.star.active {
  color: var(--star);
}
</style>
