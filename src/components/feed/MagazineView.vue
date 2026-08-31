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
  <div class="magazine-list">
    <article
      v-for="item in items"
      :key="item.id"
      class="magazine-card"
      :class="{
        faded: app.fadeRead && item.hasBeenRead,
        selected: data.selectedId === item.id,
      }"
      @click="emit('select', item)"
      @contextmenu.prevent="emit('context', $event, item)"
    >
      <!-- Thumbnail -->
      <div v-if="app.showCover && item.image" class="thumb">
        <img :src="item.image" loading="lazy" alt="" />
      </div>

      <!-- Content Area -->
      <div class="card-content">
        <div class="meta">
          <span class="source">{{ data.sourceById(item.sourceId)?.title }}</span>
          <span class="dot">·</span>
          <span class="time">{{ formatTime(item.publishedAt) }}</span>
          <div class="spacer"></div>
          <span v-if="!item.hasBeenRead" class="unread-dot"></span>
        </div>

        <h3 class="title">{{ item.title }}</h3>

        <p v-if="app.showSnippet && item.snippet" class="snippet">{{ item.snippet }}</p>

        <div class="footer">
          <span class="author">{{ item.author || '' }}</span>
          <div class="spacer"></div>
          <button
            class="f-icon-btn star-btn"
            :class="{ 'active-star': item.starred }"
            @click.stop="data.toggleStar(item)"
          >
            <Icon :name="item.starred ? 'star-fill' : 'star'" :size="15" />
          </button>
        </div>
      </div>
    </article>
  </div>
</template>

<style scoped>
.magazine-list {
  padding: 0.8rem 1.25rem;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.magazine-card {
  display: flex;
  gap: 1.1rem;
  background: var(--bg-card);
  border-radius: var(--radius-card);
  box-shadow: var(--shadow-card);
  border: 0.5px solid var(--border-card);
  padding: 0.85rem 1.1rem;
  cursor: pointer;
  transition: transform 0.22s var(--ease), box-shadow 0.22s var(--ease);
}

.magazine-card:hover {
  transform: translateY(-2px);
  box-shadow: var(--shadow-card-hover);
}

.magazine-card.selected {
  outline: 2px solid var(--accent);
  outline-offset: -1px;
}

.thumb {
  width: 10.5rem;
  aspect-ratio: 16 / 10;
  border-radius: 10px;
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

.card-content {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  justify-content: space-between;
}

.meta {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  font-size: 0.72rem;
  font-weight: 500;
  color: var(--text-tertiary);
  letter-spacing: -0.01em;
}

.source {
  color: var(--accent);
  font-weight: 600;
}

.dot {
  color: var(--text-quaternary);
}

.time {
  font-variant-numeric: tabular-nums;
}

.spacer {
  flex: 1;
}

.title {
  font-size: 1.02rem;
  font-weight: 600;
  line-height: 1.35;
  letter-spacing: -0.02em;
  margin: 0.3rem 0;
  color: var(--text-primary);
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.snippet {
  font-size: 0.82rem;
  color: var(--text-secondary);
  line-height: 1.5;
  letter-spacing: -0.01em;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  margin-bottom: 0.35rem;
}

.footer {
  display: flex;
  align-items: center;
  font-size: 0.75rem;
  color: var(--text-tertiary);
}

.author {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 12rem;
}

.star-btn {
  width: 1.8rem;
  height: 1.8rem;
  border-radius: 6px;
  color: var(--text-tertiary);
}

.star-btn:hover {
  color: var(--star);
  background: var(--bg-hover);
}
</style>
