<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { useAppStore } from './stores/app'
import { useDataStore, useUiStore } from './stores/data'
import SideNav from './components/nav/SideNav.vue'
import ArticleList from './components/feed/ArticleList.vue'
import ArticleView from './components/article/ArticleView.vue'
import SettingsPanel from './components/SettingsPanel.vue'
import AddSourceDialog from './components/AddSourceDialog.vue'
import ContextMenu from './components/ui/ContextMenu.vue'
import type { Item } from './types'

const app = useAppStore()
const data = useDataStore()
const ui = useUiStore()

const showSettings = ref(false)
const showAdd = ref(false)

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
  <div class="app-shell">
    <SideNav
      v-if="app.s.menuOn"
      @add-source="showAdd = true"
      @open-settings="showSettings = true"
    />
    <main class="main">
      <div class="panes" :class="{ 'article-open': data.selectedItem }">
        <ArticleList class="pane-list" />
        <ArticleView v-if="data.selectedItem" class="pane-article" />
      </div>
    </main>
  </div>

  <SettingsPanel v-if="showSettings" @close="showSettings = false" />
  <AddSourceDialog v-if="showAdd" @close="showAdd = false" />
  <ContextMenu />
</template>

<style scoped>
.app-shell {
  display: flex;
  height: 100vh;
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
</style>
