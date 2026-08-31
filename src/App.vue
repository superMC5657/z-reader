<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useAppStore } from './stores/app'
import { useDataStore, useUiStore } from './stores/data'
import SideNav from './components/nav/SideNav.vue'
import ArticleList from './components/feed/ArticleList.vue'
import ArticleView from './components/article/ArticleView.vue'
import SettingsPanel from './components/SettingsPanel.vue'
import AddSourceDialog from './components/AddSourceDialog.vue'
import ContextMenu from './components/ui/ContextMenu.vue'
import Icon from './components/ui/Icon.vue'
import type { Item } from './types'

const { t } = useI18n()
const app = useAppStore()
const data = useDataStore()
const ui = useUiStore()

const showSettings = ref(false)
const showAdd = ref(false)

const currentItemIndex = computed(() => {
  return data.items.findIndex((i) => i.id === data.selectedId)
})

const hasPrevArticle = computed(() => {
  return currentItemIndex.value > 0
})

const hasNextArticle = computed(() => {
  return currentItemIndex.value >= 0 && currentItemIndex.value < data.items.length - 1
})

function goToPrevArticle() {
  if (!hasPrevArticle.value) return
  const prev = data.items[currentItemIndex.value - 1]
  if (prev) data.selectItem(prev.id)
}

function goToNextArticle() {
  if (!hasNextArticle.value) return
  const next = data.items[currentItemIndex.value + 1]
  if (next) data.selectItem(next.id)
}

function matchesKey(e: KeyboardEvent, shortcutKey?: string): boolean {
  if (!shortcutKey) return false
  if (shortcutKey === 'Escape' && e.key === 'Escape') return true
  if ((shortcutKey === 'Space' || shortcutKey === ' ') && (e.key === ' ' || e.key === 'Space')) return true
  if (shortcutKey === 'Enter' && e.key === 'Enter') return true
  return e.key.toLowerCase() === shortcutKey.toLowerCase()
}

function onKeydown(e: KeyboardEvent) {
  const target = e.target as HTMLElement
  if (['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName) || target.isContentEditable) return
  if (e.ctrlKey || e.metaKey || e.altKey) return

  const sc = app.shortcuts
  const items = data.items
  const idx = items.findIndex((i) => i.id === data.selectedId)

  if (matchesKey(e, sc.nextArticle) || e.key === 'ArrowDown') {
    e.preventDefault()
    const next = Math.min(idx + 1, items.length - 1)
    if (items[next]) data.selectItem(items[next].id)
  } else if (matchesKey(e, sc.prevArticle) || e.key === 'ArrowUp') {
    e.preventDefault()
    const next = Math.max(idx - 1, 0)
    if (items[next]) data.selectItem(items[next].id)
  } else if (matchesKey(e, sc.toggleRead)) {
    const cur = data.selectedItem
    if (cur) data.setItemRead(cur, !cur.hasBeenRead)
  } else if (matchesKey(e, sc.toggleStar)) {
    const cur: Item | null = data.selectedItem
    if (cur) data.toggleStar(cur)
  } else if (matchesKey(e, sc.fetchFull)) {
    const cur = data.selectedItem
    if (cur) data.fetchFullContent(cur.id)
  } else if (matchesKey(e, sc.openInBrowser)) {
    const cur = data.selectedItem
    if (cur?.url) {
      import('@tauri-apps/plugin-opener')
        .then(({ openUrl }) => openUrl(cur.url!))
        .catch(() => window.open(cur.url!, '_blank'))
    }
  } else if (matchesKey(e, sc.refresh)) {
    data.fetchAll()
  } else if (matchesKey(e, sc.closeArticle) || e.key === 'Escape') {
    data.selectedItem = null
    data.selectedId = null
    ui.closeMenu()
  } else if (matchesKey(e, sc.addSource)) {
    showAdd.value = true
  } else if (matchesKey(e, sc.toggleSidebar)) {
    app.patch({ menuOn: !app.s.menuOn })
  }
}

onMounted(() => {
  app.init()
  data.init()
  window.addEventListener('keydown', onKeydown)
})
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeydown)
})
</script>

<template>
  <div class="app-shell" :class="{ 'blur-bg': data.selectedItem && app.isFocusMode }">
    <SideNav
      v-if="app.s.menuOn"
      @add-source="showAdd = true"
      @open-settings="showSettings = true"
    />
    <main class="main">
      <div class="panes" :class="{ 'article-open': data.selectedItem }">
        <ArticleList class="pane-list" />
        <!-- Standard Split Pane Mode -->
        <ArticleView v-if="data.selectedItem && !app.isFocusMode" class="pane-article" />
      </div>
    </main>
  </div>

  <!-- Focus Mode: Elevated Acrylic Sheet Overlay -->
  <Transition name="focus-sheet">
    <div
      v-if="data.selectedItem && app.isFocusMode"
      class="focus-backdrop"
      @click.self="data.selectedItem = null; data.selectedId = null"
    >
      <!-- Left Prev Article Button -->
      <button
        class="focus-nav-btn prev-btn"
        :class="{ disabled: !hasPrevArticle }"
        :disabled="!hasPrevArticle"
        :title="t('settings.shortcuts.prevArticle')"
        @click.stop="goToPrevArticle"
      >
        <Icon name="chevron-left" :size="20" stroke-width="2.2" />
      </button>

      <div class="focus-modal-card">
        <ArticleView is-focus-modal />
      </div>

      <!-- Right Next Article Button -->
      <button
        class="focus-nav-btn next-btn"
        :class="{ disabled: !hasNextArticle }"
        :disabled="!hasNextArticle"
        :title="t('settings.shortcuts.nextArticle')"
        @click.stop="goToNextArticle"
      >
        <Icon name="chevron-right" :size="20" stroke-width="2.2" />
      </button>
    </div>
  </Transition>

  <SettingsPanel v-if="showSettings" @close="showSettings = false" />
  <AddSourceDialog v-if="showAdd" @close="showAdd = false" />
  <ContextMenu />
</template>

<style scoped>
.app-shell {
  display: flex;
  height: 100vh;
  transition: filter 0.22s ease;
}

.main {
  flex: 1;
  min-width: 0;
  height: 100%;
}

.panes {
  display: flex;
  height: 100%;
  min-width: 0;
  overflow: hidden;
  position: relative;
}

.pane-list {
  flex: 1;
  min-width: 0;
  overflow: hidden;
}

.pane-article {
  flex: 1.15;
  min-width: 0;
  overflow: hidden;
  border-left: 0.5px solid var(--border);
}

@media (max-width: 980px) {
  .pane-article {
    position: fixed;
    inset: 0;
    z-index: 50;
  }
}

/* Acrylic Focus Mode Styles */
.focus-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.45);
  backdrop-filter: blur(24px) saturate(180%);
  -webkit-backdrop-filter: blur(24px) saturate(180%);
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 1.25rem;
  z-index: 1200;
  padding: 1.25rem 1.5rem;
}

[data-theme='dark'] .focus-backdrop {
  background: rgba(0, 0, 0, 0.62);
}

.focus-nav-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 3.2rem;
  height: 3.2rem;
  border-radius: 50%;
  background: rgba(255, 255, 255, 0.16);
  backdrop-filter: blur(24px) saturate(200%);
  -webkit-backdrop-filter: blur(24px) saturate(200%);
  border: 0.5px solid rgba(255, 255, 255, 0.35);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.22), inset 0 0.5px 0.5px rgba(255, 255, 255, 0.5);
  color: #ffffff;
  cursor: pointer;
  outline: none;
  flex-shrink: 0;
  transition: all 0.22s cubic-bezier(0.16, 1, 0.3, 1);
  user-select: none;
}

.focus-nav-btn:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.28);
  border-color: rgba(255, 255, 255, 0.6);
  box-shadow: 0 12px 36px rgba(0, 0, 0, 0.28), inset 0 0.5px 0.5px rgba(255, 255, 255, 0.65);
  transform: scale(1.1);
}

.focus-nav-btn:active:not(:disabled) {
  transform: scale(0.95);
  background: rgba(255, 255, 255, 0.22);
}

.focus-nav-btn:disabled,
.focus-nav-btn.disabled {
  opacity: 0.2;
  cursor: not-allowed;
  pointer-events: none;
  transform: none !important;
  box-shadow: none;
}

[data-theme='dark'] .focus-nav-btn {
  background: rgba(45, 45, 48, 0.65);
  border-color: rgba(255, 255, 255, 0.18);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.45), inset 0 0.5px 0.5px rgba(255, 255, 255, 0.28);
  color: #f5f5f7;
}

[data-theme='dark'] .focus-nav-btn:hover:not(:disabled) {
  background: rgba(70, 70, 75, 0.85);
  border-color: rgba(255, 255, 255, 0.35);
  color: #ffffff;
}

@media (max-width: 1040px) {
  .focus-backdrop {
    padding: 1rem 0.5rem;
    gap: 0.5rem;
  }

  .focus-nav-btn {
    width: 2.6rem;
    height: 2.6rem;
  }
}

.focus-modal-card {
  width: 920px;
  max-width: 94vw;
  height: 90vh;
  max-height: 92vh;
  background: var(--bg);
  border-radius: 16px;
  box-shadow: 0 24px 64px rgba(0, 0, 0, 0.4), 0 0 0 1px var(--border);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.focus-sheet-enter-active,
.focus-sheet-leave-active {
  transition: opacity 0.22s cubic-bezier(0.16, 1, 0.3, 1);
}

.focus-sheet-enter-active .focus-modal-card,
.focus-sheet-leave-active .focus-modal-card {
  transition: transform 0.26s cubic-bezier(0.16, 1, 0.3, 1), opacity 0.22s ease;
}

.focus-sheet-enter-from,
.focus-sheet-leave-to {
  opacity: 0;
}

.focus-sheet-enter-from .focus-modal-card,
.focus-sheet-leave-to .focus-modal-card {
  opacity: 0;
  transform: scale(0.95) translateY(12px);
}
</style>
