<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { openUrl } from '@tauri-apps/plugin-opener'
import { useDataStore, useUiStore } from '../../stores/data'
import { useAppStore } from '../../stores/app'
import type { Item } from '../../types'
import CardsView from './CardsView.vue'
import ListView from './ListView.vue'
import MagazineView from './MagazineView.vue'
import CompactView from './CompactView.vue'

const { t } = useI18n()
const data = useDataStore()
const app = useAppStore()

const scopeTitle = computed(() => {
  if (data.scope.type === 'all') return t('nav.all')
  if (data.scope.type === 'group') {
    return data.groups.find((g) => g.id === data.scope.id)?.name ?? ''
  }
  return data.sourceById(data.scope.id ?? -1)?.title ?? ''
})

const filterTabs = computed(() => [
  { value: 0, label: t('filter.all') },
  { value: 1, label: t('filter.unread') },
  { value: 2, label: t('filter.starred') },
])

const views = ['cards', 'list', 'magazine', 'compact'] as const
const viewIcons: Record<string, string> = { cards: '▦', list: '☰', magazine: '▤', compact: '▥' }

const activeView = computed(() => app.s.view)

function onContext(e: MouseEvent, item: Item) {
  data.selectItem(item.id)
  itemContextMenu(e, item)
}

const ui = useUiStore()

function itemContextMenu(e: MouseEvent, item: Item) {
  ui.openMenu(e.clientX, e.clientY, [
    {
      label: t(item.hasBeenRead ? 'item.markUnread' : 'item.markRead'),
      action: () => data.setItemRead(item, !item.hasBeenRead),
    },
    { label: t(item.starred ? 'item.unstar' : 'item.star'), action: () => data.toggleStar(item) },
    {
      label: t('item.openWeb'),
      action: () => item.url && openUrl(item.url),
    },
  ])
}
</script>

<template>
  <section class="article-list">
    <header class="toolbar">
      <div class="scope">
        <h2>{{ scopeTitle }}</h2>
        <span v-if="data.unreadOf(data.scope)" class="count">{{ data.unreadOf(data.scope) }}</span>
      </div>

      <div class="segmented">
        <button
          v-for="tab in filterTabs"
          :key="tab.value"
          class="seg"
          :class="{ active: app.s.filterType === tab.value }"
          @click="data.setFilter(tab.value)"
        >
          {{ tab.label }}
        </button>
      </div>

      <div class="spacer"></div>

      <input
        class="search"
        :placeholder="t('toolbar.search')"
        :value="data.search"
        @input="data.search_(($event.target as HTMLInputElement).value)"
      />

      <button class="f-icon-btn" :title="t('toolbar.markAllRead')" @click="data.markAllReadInScope()">✓</button>

      <div class="segmented view-switch">
        <button
          v-for="v in views"
          :key="v"
          class="seg"
          :class="{ active: activeView === v }"
          :title="t(`toolbar.views.${v}`)"
          @click="app.patch({ view: v })"
        >
          {{ viewIcons[v] }}
        </button>
      </div>
    </header>

    <div class="list-body">
      <div v-if="data.loading && !data.items.length" class="state">{{ t('common.loading') }}</div>
      <div v-else-if="!data.items.length" class="state">
        <p>{{ t('common.empty') }}</p>
        <p class="sub">{{ t('common.emptyHint') }}</p>
      </div>
      <component
        :is="{ cards: CardsView, list: ListView, magazine: MagazineView, compact: CompactView }[activeView]"
        v-else
        :items="data.items"
        @select="(item: Item) => data.selectItem(item.id)"
        @context="onContext"
      />
    </div>
  </section>
</template>

<style scoped>
.article-list {
  height: 100%;
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.toolbar {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  padding: 0.55rem 1rem;
  border-bottom: 1px solid var(--border);
  flex-wrap: nowrap;
}

.scope {
  display: flex;
  align-items: baseline;
  gap: 0.45rem;
  min-width: 0;
}

.scope h2 {
  font-size: 1.05rem;
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 12rem;
}

.count {
  font-size: 0.75rem;
  color: var(--text-secondary);
}

.segmented {
  display: inline-flex;
  background: var(--bg-active);
  border-radius: 5px;
  padding: 2px;
}

.seg {
  padding: 0.2rem 0.65rem;
  border-radius: 4px;
  font-size: 0.8rem;
  color: var(--text-secondary);
}

.seg.active {
  background: var(--bg-card);
  color: var(--text-primary);
  box-shadow: var(--shadow-card);
}

.spacer {
  flex: 1;
}

.search {
  width: 11rem;
  font-size: 0.82rem;
  border-radius: 5px;
}

.list-body {
  flex: 1;
  overflow-y: auto;
}

.state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 100%;
  color: var(--text-tertiary);
  gap: 0.3rem;
}

.state .sub {
  font-size: 0.82rem;
}
</style>
