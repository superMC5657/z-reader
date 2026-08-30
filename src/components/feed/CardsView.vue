<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { useDataStore } from '../../stores/data'
import { useAppStore } from '../../stores/app'
import { formatTime } from '../../lib/time'
import type { Item } from '../../types'

defineProps<{ items: Item[] }>()
const emit = defineEmits<{ select: [Item]; context: [MouseEvent, Item] }>()

const { t } = useI18n()
const data = useDataStore()
const app = useAppStore()
</script>

<template>
  <div class="cards-grid">
    <article
      v-for="item in items"
      :key="item.id"
      class="card"
      :class="{ faded: app.fadeRead && item.hasBeenRead, selected: data.selectedId === item.id }"
      @click="emit('select', item)"
      @contextmenu.prevent="emit('context', $event, item)"
    >
      <div v-if="app.showCover && item.image" class="cover">
        <img :src="item.image" loading="lazy" alt="" />
      </div>
      <div class="card-body">
        <div class="meta">
          <span class="source-name">{{ data.sourceById(item.sourceId)?.title }}</span>
          <span class="dot">·</span>
          <span>{{ formatTime(item.publishedAt) }}</span>
          <div class="spacer"></div>
          <span v-if="!item.hasBeenRead" class="unread-dot" :title="t('filter.unread')"></span>
        </div>
        <h3 class="title">{{ item.title }}</h3>
        <p v-if="app.showSnippet && item.snippet" class="snippet">{{ item.snippet }}</p>
        <div class="card-footer">
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
.cards-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
  gap: 0.8rem;
  padding: 0.9rem 1rem;
}

.card {
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  box-shadow: var(--shadow-card);
  overflow: hidden;
  cursor: pointer;
  transition: transform 0.12s, box-shadow 0.12s;
  display: flex;
  flex-direction: column;
}

.card:hover {
  transform: translateY(-2px);
  box-shadow: var(--shadow-flyout);
}

.card.selected {
  outline: 2px solid var(--accent);
}

.cover {
  aspect-ratio: 16 / 9;
  background: var(--bg-hover);
}

.cover img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}

.card-body {
  padding: 0.7rem 0.85rem;
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
  flex: 1;
}

.meta {
  display: flex;
  align-items: center;
  gap: 0.3rem;
  font-size: 0.72rem;
  color: var(--text-tertiary);
}

.source-name {
  color: var(--accent);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.spacer {
  flex: 1;
}

.title {
  font-size: 0.95rem;
  font-weight: 600;
  line-height: 1.35;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.snippet {
  font-size: 0.8rem;
  color: var(--text-secondary);
  line-height: 1.45;
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.card-footer {
  display: flex;
  align-items: center;
  margin-top: auto;
  font-size: 0.75rem;
  color: var(--text-tertiary);
}

.star {
  width: 1.6rem;
  height: 1.6rem;
}

.star.active {
  color: #f2b705;
}
</style>
