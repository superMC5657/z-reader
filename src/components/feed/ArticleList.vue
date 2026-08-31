<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { openUrl } from '@tauri-apps/plugin-opener'
import { useDataStore, useUiStore } from '../../stores/data'
import { useAppStore } from '../../stores/app'
import type { Item } from '../../types'
import Icon from '../ui/Icon.vue'
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
const viewIcons: Record<string, string> = {
  cards: 'view-cards',
  list: 'view-list',
  magazine: 'view-magazine',
  compact: 'view-compact',
}

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

function clearSearch() {
  data.search_('')
}

function openViewMenu(e: MouseEvent) {
  const target = e.currentTarget as HTMLElement
  const rect = target.getBoundingClientRect()
  ui.openMenu(rect.right - 180, rect.bottom + 6, [
    {
      label: t('toolbar.views.cards'),
      icon: 'view-cards',
      checked: activeView.value === 'cards',
      action: () => app.patch({ view: 'cards' }),
    },
    {
      label: t('toolbar.views.list'),
      icon: 'view-list',
      checked: activeView.value === 'list',
      action: () => app.patch({ view: 'list' }),
    },
    {
      label: t('toolbar.views.magazine'),
      icon: 'view-magazine',
      checked: activeView.value === 'magazine',
      action: () => app.patch({ view: 'magazine' }),
    },
    {
      label: t('toolbar.views.compact'),
      icon: 'view-compact',
      checked: activeView.value === 'compact',
      action: () => app.patch({ view: 'compact' }),
    },
  ])
}
</script>

<template>
  <section class="article-list">
    <!-- Apple macOS Unified Toolbar -->
    <header class="toolbar" data-tauri-drag-region>
      <div class="scope" data-tauri-drag-region>
        <h2 data-tauri-drag-region>{{ scopeTitle }}</h2>
        <span v-if="data.unreadOf(data.scope)" class="count">
          {{ data.unreadOf(data.scope) }}
        </span>
      </div>

      <!-- Segmented Filter Control -->
      <div class="segmented filter-seg">
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

      <div class="spacer" data-tauri-drag-region></div>

      <!-- Right Actions Group -->
      <div class="toolbar-actions">
        <!-- Apple Spotlight Capsule Search Box -->
        <div class="search-wrapper" :class="{ 'has-search': !!data.search }">
          <Icon name="search" :size="13" color="var(--text-tertiary)" class="search-icon" />
          <input
            class="search-input"
            :placeholder="t('toolbar.search')"
            :value="data.search"
            @input="data.search_(($event.target as HTMLInputElement).value)"
          />
          <button v-if="data.search" class="clear-btn" @click="clearSearch" title="Clear">
            <Icon name="xmark" :size="14" color="var(--text-tertiary)" />
          </button>
        </div>

        <!-- Mark All Read -->
        <button
          class="f-icon-btn toolbar-btn"
          :title="t('toolbar.markAllRead')"
          @click="data.markAllReadInScope()"
        >
          <Icon name="checkmark-circle" :size="17" />
        </button>

        <!-- View Switcher: Segmented Control (Wide mode >= 640px) -->
        <div class="segmented view-switch wide-view-switch">
          <button
            v-for="v in views"
            :key="v"
            class="seg view-seg"
            :class="{ active: activeView === v }"
            :title="t(`toolbar.views.${v}`)"
            @click="app.patch({ view: v })"
          >
            <Icon :name="viewIcons[v]" :size="14" />
          </button>
        </div>

        <!-- View Switcher: Single Dropdown Button (Compact mode < 640px) -->
        <button
          class="f-icon-btn toolbar-btn compact-view-btn"
          :title="t(`toolbar.views.${activeView}`)"
          @click="openViewMenu"
        >
          <Icon :name="viewIcons[activeView]" :size="15" />
        </button>
      </div>
    </header>

    <!-- Main List Body -->
    <div class="list-body">
      <!-- Loading State -->
      <div v-if="data.loading && !data.items.length" class="state">
        <Icon name="arrow-clockwise" :size="28" color="var(--accent)" class="spin" />
        <p>{{ t('common.loading') }}</p>
      </div>

      <!-- Empty State -->
      <div v-else-if="!data.items.length" class="state">
        <div class="empty-icon-circle">
          <Icon name="tray-stack" :size="32" color="var(--text-quaternary)" />
        </div>
        <p class="empty-title">{{ t('common.empty') }}</p>
        <p class="sub">{{ t('common.emptyHint') }}</p>
      </div>

      <!-- Feed Views -->
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
  overflow: hidden;
  background: var(--bg);
  container-type: inline-size;
  container-name: article-list;
}

.toolbar {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 0.65rem 1.25rem;
  background: var(--bg-toolbar);
  backdrop-filter: var(--glass-blur);
  -webkit-backdrop-filter: var(--glass-blur);
  border-bottom: 0.5px solid var(--border);
  flex-wrap: nowrap;
  min-height: 52px;
  min-width: 0;
  width: 100%;
  overflow: hidden;
  z-index: 10;
}

.scope {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  min-width: 0;
  flex-shrink: 1;
}

.scope h2 {
  font-size: 1.18rem;
  font-weight: 700;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 14rem;
  letter-spacing: -0.025em;
  min-width: 0;
}

.count {
  font-size: 0.76rem;
  font-weight: 600;
  color: var(--text-secondary);
  background: var(--bg-track);
  padding: 0.1rem 0.5rem;
  border-radius: var(--radius-pill);
  font-variant-numeric: tabular-nums;
  flex-shrink: 0;
}

.filter-seg {
  flex-shrink: 0;
}

.spacer {
  flex: 1;
  min-width: 0.25rem;
}

.toolbar-actions {
  display: flex;
  align-items: center;
  gap: 0.45rem;
  flex-shrink: 0;
  min-width: 0;
}

/* Apple Capsule Search */
.search-wrapper {
  position: relative;
  display: flex;
  align-items: center;
  width: 11rem;
  min-width: 4.5rem;
  flex-shrink: 1;
  transition: width 0.2s var(--ease);
}

.search-icon {
  position: absolute;
  left: 0.65rem;
  pointer-events: none;
}

.search-input {
  width: 100%;
  font-size: 0.82rem;
  border-radius: var(--radius-pill);
  background: var(--bg-input);
  padding: 0.32rem 1.8rem 0.32rem 1.85rem;
  border: 1px solid var(--border-subtle);
  transition: all 0.2s var(--ease);
}

.search-input:focus {
  background: var(--bg-input-focus);
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-tint);
}

.search-input::placeholder {
  color: var(--text-tertiary);
}

.clear-btn {
  position: absolute;
  right: 0.5rem;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 2px;
  border-radius: 50%;
  transition: opacity 0.15s ease;
}

.clear-btn:hover {
  opacity: 0.75;
}

.toolbar-btn {
  border-radius: 8px;
  color: var(--text-secondary);
  flex-shrink: 0;
}

.toolbar-btn:hover {
  color: var(--accent);
  background: var(--bg-hover-strong);
}

.wide-view-switch {
  display: inline-flex;
  flex-shrink: 0;
}

.compact-view-btn {
  display: none;
}

.view-seg {
  padding: 0.25rem 0.55rem;
}

/* Responsive Container Queries for ArticleList */
@container article-list (max-width: 640px) {
  .toolbar {
    padding: 0.55rem 0.85rem;
    gap: 0.45rem;
  }

  .wide-view-switch {
    display: none;
  }

  .compact-view-btn {
    display: inline-flex;
  }

  .search-wrapper {
    width: 8rem;
  }

  .scope h2 {
    max-width: 9rem;
    font-size: 1.05rem;
  }
}

@container article-list (max-width: 480px) {
  .toolbar {
    padding: 0.5rem 0.65rem;
    gap: 0.35rem;
  }

  .toolbar-actions {
    gap: 0.25rem;
  }

  .scope h2 {
    max-width: 6.5rem;
    font-size: 0.98rem;
  }

  .filter-seg .seg {
    padding: 0.2rem 0.5rem;
    font-size: 0.78rem;
  }

  .search-wrapper {
    width: 2rem;
  }

  .search-wrapper:focus-within,
  .search-wrapper.has-search {
    width: 7.5rem;
  }

  .search-wrapper:not(:focus-within):not(.has-search) .search-input {
    cursor: pointer;
    background: transparent;
    border-color: transparent;
    color: transparent;
    padding-left: 0.65rem;
  }

  .search-wrapper:not(:focus-within):not(.has-search) .search-input::placeholder {
    color: transparent;
  }

  .search-wrapper:not(:focus-within):not(.has-search):hover {
    background: var(--bg-hover);
    border-radius: var(--radius-pill);
  }
}

@container article-list (max-width: 360px) {
  .scope h2 {
    max-width: 4.5rem;
  }

  .filter-seg .seg {
    padding: 0.18rem 0.38rem;
    font-size: 0.74rem;
  }
}

.list-body {
  flex: 1;
  overflow-y: auto;
  position: relative;
}

.state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 100%;
  color: var(--text-tertiary);
  gap: 0.55rem;
  font-size: 0.92rem;
}

.empty-icon-circle {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 4rem;
  height: 4rem;
  border-radius: 50%;
  background: var(--bg-track);
  margin-bottom: 0.35rem;
}

.empty-title {
  font-weight: 600;
  color: var(--text-secondary);
  font-size: 1rem;
}

.state .sub {
  font-size: 0.8rem;
  color: var(--text-tertiary);
}
</style>
