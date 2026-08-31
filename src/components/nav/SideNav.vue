<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { convertFileSrc } from '@tauri-apps/api/core'
import { useDataStore, useUiStore } from '../../stores/data'
import * as api from '../../lib/tauri'
import type { Source } from '../../types'
import Modal from '../ui/Modal.vue'

const { t } = useI18n()
const data = useDataStore()
const ui = useUiStore()

const showGroupModal = ref(false)
const groupModalTitle = ref('')
const groupModalName = ref('')
let groupModalTarget: number | null = null // null = create new

const ungroupedSources = computed(() => data.sources.filter((s) => s.groupId === null))
const sourcesOf = computed(() => {
  const map = new Map<number, Source[]>()
  for (const g of data.groups) map.set(g.id, [])
  for (const s of data.sources) {
    if (s.groupId !== null) map.get(s.groupId)?.push(s)
  }
  return (gid: number) => map.get(gid) ?? []
})

function faviconSrc(source: Source): string | null {
  return source.favicon ? convertFileSrc(source.favicon) : null
}

function sourceMenu(e: MouseEvent, s: Source) {
  const groups = data.groups
  ui.openMenu(e.clientX, e.clientY, [
    { label: t('item.markAllRead'), action: () => api.markAllRead('source', s.id).then(() => data.loadSources()) },
    { label: t('group.rename'), action: () => openGroupModal(s.id, s.title) },
    ...groups.map((g) => ({
      label: `${t('group.moveTo')} · ${g.name}`,
      action: () => api.setSourceGroup(s.id, g.id).then(() => Promise.all([data.loadSources(), data.loadGroups()])),
    })),
    ...(s.groupId !== null
      ? [
          {
            label: t('settings.sources.ungrouped'),
            action: () => api.setSourceGroup(s.id, null).then(() => Promise.all([data.loadSources(), data.loadGroups()])),
          },
        ]
      : []),
    {
      label: t('settings.sources.remove'),
      danger: true,
      action: () => {
        if (confirm(t('settings.sources.confirmRemove'))) data.removeSource(s.id)
      },
    },
  ])
}

function groupMenu(e: MouseEvent, gid: number) {
  ui.openMenu(e.clientX, e.clientY, [
    { label: t('item.markAllRead'), action: () => api.markAllRead('group', gid).then(() => Promise.all([data.loadSources(), data.loadItems()])) },
    { label: t('group.rename'), action: () => openGroupModal(gid, data.groups.find((g) => g.id === gid)?.name ?? '') },
    { label: t('group.delete'), danger: true, action: () => { if (confirm(t('group.confirmDelete'))) api.deleteGroup(gid).then(() => Promise.all([data.loadSources(), data.loadGroups(), data.loadItems()])) } },
  ])
}

function openGroupModal(targetId: number | null, initialName: string) {
  groupModalTarget = targetId
  groupModalName.value = initialName
  groupModalTitle.value = targetId === null ? t('group.new') : t('group.rename')
  showGroupModal.value = true
}

async function submitGroupModal() {
  const name = groupModalName.value.trim()
  if (!name) return
  if (groupModalTarget === null) {
    await api.createGroup(name)
  } else {
    await api.renameGroup(groupModalTarget, name)
  }
  showGroupModal.value = false
  await Promise.all([data.loadGroups(), data.loadSources()])
}

async function toggleExpand(gid: number) {
  const g = data.groups.find((x) => x.id === gid)
  if (!g) return
  g.expanded = !g.expanded
  await api.setGroupExpanded(gid, g.expanded)
}
</script>

<template>
  <nav class="side-nav">
    <div class="nav-header">
      <span class="logo">Z</span>
      <span class="app-name">ZReader</span>
    </div>

    <div class="nav-body">
      <button
        class="nav-row all"
        :class="{ active: data.scope.type === 'all' }"
        @click="data.selectScope('all')"
      >
        <span class="row-icon">☰</span>
        <span class="row-title">{{ t('nav.all') }}</span>
        <span v-if="data.totalUnread" class="unread-badge">{{ data.totalUnread }}</span>
      </button>

      <template v-for="g in data.groups" :key="g.id">
        <div class="nav-row group" :class="{ active: data.scope.type === 'group' && data.scope.id === g.id }">
          <button class="chevron" @click="toggleExpand(g.id)">{{ g.expanded ? '▾' : '▸' }}</button>
          <button class="row-main" @click="data.selectScope('group', g.id)" @contextmenu.prevent="groupMenu($event, g.id)">
            <span class="row-icon">📁</span>
            <span class="row-title">{{ g.name }}</span>
            <span v-if="data.unreadOf({ type: 'group', id: g.id })" class="unread-badge">
              {{ data.unreadOf({ type: 'group', id: g.id }) }}
            </span>
          </button>
        </div>
        <template v-if="g.expanded">
          <button
            v-for="s in sourcesOf(g.id)"
            :key="s.id"
            class="nav-row source"
            :class="{ active: data.scope.type === 'source' && data.scope.id === s.id, error: s.errorCount > 0 }"
            @click="data.selectScope('source', s.id)"
            @contextmenu.prevent="sourceMenu($event, s)"
          >
            <img v-if="faviconSrc(s)" class="favicon" :src="faviconSrc(s)!" alt="" />
            <span v-else class="favicon placeholder"></span>
            <span class="row-title">{{ s.title }}</span>
            <span v-if="s.unread" class="unread-badge">{{ s.unread }}</span>
          </button>
        </template>
      </template>

      <div v-if="ungroupedSources.length" class="divider"></div>

      <button
        v-for="s in ungroupedSources"
        :key="s.id"
        class="nav-row source"
        :class="{ active: data.scope.type === 'source' && data.scope.id === s.id, error: s.errorCount > 0 }"
        @click="data.selectScope('source', s.id)"
        @contextmenu.prevent="sourceMenu($event, s)"
      >
        <img v-if="faviconSrc(s)" class="favicon" :src="faviconSrc(s)!" alt="" />
        <span v-else class="favicon placeholder"></span>
        <span class="row-title">{{ s.title }}</span>
        <span v-if="s.unread" class="unread-badge">{{ s.unread }}</span>
      </button>

      <div v-if="!data.sources.length" class="empty-hint">
        <p>{{ t('nav.noSources') }}</p>
        <p class="sub">{{ t('nav.addHint') }}</p>
      </div>
    </div>

    <div class="nav-footer">
      <button class="f-icon-btn" :title="t('nav.addSource')" @click="$emit('add-source')">＋</button>
      <button class="f-icon-btn" :title="t('group.new')" @click="openGroupModal(null, '')">🗂</button>
      <button class="f-icon-btn" :class="{ spinning: data.fetching }" :title="t('nav.refresh')" @click="data.fetchAll()">
        <span :class="{ spin: data.fetching }">⟳</span>
      </button>
      <div class="spacer"></div>
      <button class="f-icon-btn" :title="t('nav.settings')" @click="$emit('open-settings')">⚙</button>
    </div>
  </nav>

  <Modal v-if="showGroupModal" :title="groupModalTitle" @close="showGroupModal = false">
    <div class="form-row">
      <label>{{ t('group.namePlaceholder') }}</label>
      <input v-model="groupModalName" autofocus @keyup.enter="submitGroupModal" />
    </div>
    <template #footer>
      <button class="f-btn" @click="showGroupModal = false">{{ t('common.cancel') }}</button>
      <button class="f-btn primary" @click="submitGroupModal">{{ t('common.confirm') }}</button>
    </template>
  </Modal>
</template>

<style scoped>
.side-nav {
  width: var(--nav-width);
  height: 100%;
  display: flex;
  flex-direction: column;
  background: var(--bg-sidebar);
  backdrop-filter: blur(28px) saturate(1.6);
  border-right: 0.5px solid var(--border);
  flex-shrink: 0;
}

.nav-header {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 1rem 1.1rem 0.7rem;
  font-weight: 700;
  font-size: 0.95rem;
  letter-spacing: -0.02em;
}

.logo {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 1.5rem;
  height: 1.5rem;
  border-radius: 7px;
  background: linear-gradient(180deg, #3ea1ff, var(--accent));
  color: #fff;
  font-weight: 800;
  font-size: 0.85rem;
  box-shadow: 0 1px 2px rgba(0, 122, 255, 0.35);
}

.nav-body {
  flex: 1;
  overflow-y: auto;
  padding: 0 0.5rem 0.4rem;
}

.nav-row {
  display: flex;
  align-items: center;
  gap: 0.45rem;
  width: 100%;
  padding: 0.32rem 0.6rem;
  border-radius: 7px;
  text-align: left;
  color: var(--text-primary);
  transition: background 0.15s var(--ease);
}

.nav-row:hover {
  background: var(--bg-hover);
}

.nav-row.all.active,
.nav-row.group.active .row-main {
  background: var(--accent);
  color: #fff;
}

.nav-row.all.active .row-icon,
.nav-row.group.active .row-main .row-icon {
  color: #fff;
}

.nav-row.all.active .unread-badge,
.nav-row.group.active .row-main .unread-badge {
  background: rgba(255, 255, 255, 0.25);
  color: #fff;
}

.nav-row.group {
  padding-left: 0.3rem;
}

.nav-row.source {
  padding-left: 1.7rem;
  font-size: 0.86rem;
  color: var(--text-secondary);
}

.nav-row.source.active {
  background: var(--accent);
  color: #fff;
}

.nav-row.source.active .row-title {
  color: #fff;
}

.nav-row.source.active .unread-badge {
  background: rgba(255, 255, 255, 0.25);
  color: #fff;
}

.nav-row.error .row-title {
  color: var(--danger);
}

.nav-row.error.active .row-title {
  color: #ffd6d3;
}

.chevron {
  width: 1.2rem;
  height: 1.6rem;
  color: var(--text-tertiary);
  font-size: 0.62rem;
  transition: transform 0.15s var(--ease);
}

.row-main {
  display: flex;
  align-items: center;
  gap: 0.45rem;
  flex: 1;
  min-width: 0;
  padding: 0.32rem 0.6rem;
  border-radius: 7px;
  margin: 0 -0.15rem;
  text-align: left;
}

.row-icon {
  width: 1.15rem;
  text-align: center;
  flex-shrink: 0;
  color: var(--accent);
  font-size: 0.85rem;
}

.row-title {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.favicon {
  width: 1rem;
  height: 1rem;
  border-radius: 3.5px;
  flex-shrink: 0;
}

.favicon.placeholder {
  background: var(--text-tertiary);
  opacity: 0.35;
}

.divider {
  height: 0.5px;
  background: var(--border);
  margin: 0.45rem 0.6rem;
}

.empty-hint {
  padding: 1.4rem 1rem;
  color: var(--text-tertiary);
  font-size: 0.82rem;
  text-align: center;
}

.empty-hint .sub {
  font-size: 0.76rem;
  margin-top: 0.3rem;
}

.nav-footer {
  display: flex;
  align-items: center;
  gap: 0.1rem;
  padding: 0.45rem 0.7rem calc(0.5rem);
  border-top: 0.5px solid var(--border);
}

.nav-footer .f-icon-btn {
  color: var(--accent);
  font-size: 0.95rem;
}

.spacer {
  flex: 1;
}
</style>
