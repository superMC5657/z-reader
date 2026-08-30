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

function onKeydown(e: KeyboardEvent) {
  const target = e.target as HTMLElement
  if (['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName)) return
  const items = data.items
  const idx = items.findIndex((i) => i.id === data.selectedId)
  if (e.key === 'j' || e.key === 'k') {
    e.preventDefault()
    const next = e.key === 'j' ? Math.min(idx + 1, items.length - 1) : Math.max(idx - 1, 0)
    if (items[next]) data.selectItem(items[next].id)
  } else if (e.key === 'm') {
    const cur = data.selectedItem
    if (cur) data.setItemRead(cur, !cur.hasBeenRead)
  } else if (e.key === 's') {
    const cur: Item | null = data.selectedItem
    if (cur) data.toggleStar(cur)
  } else if (e.key === 'r') {
    data.fetchAll()
  } else if (e.key === 'Escape') {
    data.selectedItem = null
    data.selectedId = null
    ui.closeMenu()
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
}

.pane-list {
  flex: 1;
  min-width: 0;
}

.pane-article {
  flex: 1.15;
  border-left: 1px solid var(--border);
}

@media (max-width: 980px) {
  .pane-article {
    position: fixed;
    inset: 0;
    z-index: 50;
  }
}
</style>
