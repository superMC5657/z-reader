<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { useDataStore } from '../../stores/data'
import { useAppStore } from '../../stores/app'
import { formatTime } from '../../lib/time'
import type { Item } from '../../types'
import Icon from '../ui/Icon.vue'

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
      :class="{
        faded: app.fadeRead && item.hasBeenRead,
        selected: data.selectedId === item.id,
      }"
      @click="emit('select', item)"
      @contextmenu.prevent="emit('context', $event, item)"
    >
      <!-- Cover Image -->
      <div v-if="app.showCover && item.image" class="cover">
        <img :src="item.image" loading="lazy" alt="" />
        <span v-if="!item.hasBeenRead" class="unread-dot floating-dot" :title="t('filter.unread')"></span>
      </div>

      <!-- Card Body -->
      <div class="card-body">
        <div class="meta">
          <span class="source-name">{{ data.sourceById(item.sourceId)?.title }}</span>
          <span class="dot">·</span>
          <span class="time">{{ formatTime(item.publishedAt) }}</span>
          <div class="spacer"></div>
          <span
            v-if="!item.hasBeenRead && !(app.showCover && item.image)"
            class="unread-dot"
            :title="t('filter.unread')"
          ></span>
        </div>

        <h3 class="title">{{ item.title }}</h3>

        <p v-if="app.showSnippet && item.snippet" class="snippet">{{ item.snippet }}</p>

        <div class="card-footer">
          <span class="author">{{ item.author || '' }}</span>
          <div class="spacer"></div>
          <button
            class="f-icon-btn star-btn"
            :class="{ 'active-star': item.starred }"
            :title="t(item.starred ? 'item.unstar' : 'item.star')"
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
.cards-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
  gap: 1rem;
  padding: 1.1rem 1.25rem;
}

.card {
  background: var(--bg-card);
  border-radius: var(--radius-card);
  box-shadow: var(--shadow-card);
  border: 0.5px solid var(--border-card);
  overflow: hidden;
  cursor: pointer;
  transition: transform 0.22s var(--ease), box-shadow 0.22s var(--ease), border-color 0.2s ease;
  display: flex;
  flex-direction: column;
  position: relative;
}

.card:hover {
  transform: translateY(-2.5px);
  box-shadow: var(--shadow-card-hover);
}

.card.selected {
  outline: 2px solid var(--accent);
  outline-offset: -1px;
}

.cover {
  aspect-ratio: 16 / 9.5;
  background: var(--bg-track);
  position: relative;
  overflow: hidden;
}

.cover img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
  transition: transform 0.3s var(--ease);
}

.card:hover .cover img {
  transform: scale(1.02);
}

.floating-dot {
  position: absolute;
  top: 0.6rem;
  right: 0.6rem;
  box-shadow: 0 0 8px rgba(0, 0, 0, 0.4), 0 0 6px var(--accent);
}

.card-body {
  padding: 0.85rem 0.95rem 0.65rem;
  display: flex;
  flex-direction: column;
  gap: 0.45rem;
  flex: 1;
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

.source-name {
  color: var(--accent);
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 10rem;
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
  font-size: 0.96rem;
  font-weight: 600;
  line-height: 1.38;
  letter-spacing: -0.02em;
  color: var(--text-primary);
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.snippet {
  font-size: 0.8rem;
  color: var(--text-secondary);
  line-height: 1.5;
  letter-spacing: -0.01em;
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.card-footer {
  display: flex;
  align-items: center;
  margin-top: auto;
  padding-top: 0.35rem;
  font-size: 0.75rem;
  color: var(--text-tertiary);
}

.author {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 9rem;
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
