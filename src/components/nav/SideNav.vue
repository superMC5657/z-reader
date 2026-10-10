<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDataStore, useUiStore } from '../../stores/data'
import * as api from '../../lib/tauri'
import type { Source } from '../../types'
import Modal from '../ui/Modal.vue'
import Icon from '../ui/Icon.vue'
import FeedIcon from '../ui/FeedIcon.vue'
import TrafficLights from '../ui/TrafficLights.vue'
import EmptyState from '../ui/EmptyState.vue'
import ConfirmModal from '../ui/ConfirmModal.vue'

defineEmits<{
  'add-source': []
  'open-settings': []
}>()

const { t } = useI18n()
const data = useDataStore()
const ui = useUiStore()

const showGroupModal = ref(false)
const groupModalTitle = ref('')
const groupModalName = ref('')
let groupModalTarget: number | null = null // null = 新建分组
let modalMode: 'createGroup' | 'renameGroup' | 'renameSource' = 'createGroup'

const confirmState = ref<{
  show: boolean
  title?: string
  message: string
  confirmText?: string
  danger?: boolean
  loading?: boolean
  onConfirm?: () => void | Promise<void>
}>({
  show: false,
  message: '',
})

function requestConfirm(options: {
  title?: string
  message: string
  confirmText?: string
  danger?: boolean
  onConfirm: () => void | Promise<void>
}) {
  confirmState.value = {
    show: true,
    title: options.title,
    message: options.message,
    confirmText: options.confirmText,
    danger: options.danger ?? true,
    loading: false,
    onConfirm: options.onConfirm,
  }
}

async function handleConfirm() {
  if (confirmState.value.onConfirm) {
    confirmState.value.loading = true
    try {
      await confirmState.value.onConfirm()
    } finally {
      confirmState.value.loading = false
      confirmState.value.show = false
    }
  } else {
    confirmState.value.show = false
  }
}

const ungroupedSources = computed(() => data.sources.filter((s) => s.groupId === null))
const sourcesOf = computed(() => {
  const map = new Map<number, Source[]>()
  for (const g of data.groups) map.set(g.id, [])
  for (const s of data.sources) {
    if (s.groupId !== null) map.get(s.groupId)?.push(s)
  }
  return (gid: number) => map.get(gid) ?? []
})

const iconFileInput = ref<HTMLInputElement | null>(null)
let customIconTargetSourceId: number | null = null

function triggerCustomIconUpload(sourceId: number) {
  customIconTargetSourceId = sourceId
  iconFileInput.value?.click()
}

async function onIconFileSelected(e: Event) {
  const file = (e.target as HTMLInputElement).files?.[0]
  if (!file || customIconTargetSourceId === null) return
  const sid = customIconTargetSourceId
  const reader = new FileReader()
  reader.onload = async () => {
    const base64 = reader.result as string
    if (base64) {
      await api.setCustomFavicon(sid, base64)
      await data.loadSources()
    }
  }
  reader.readAsDataURL(file)
  ;(e.target as HTMLInputElement).value = ''
}

async function reFetchFavicon(sourceId: number) {
  await api.refreshFavicon(sourceId)
  await data.loadSources()
}

function sourceMenu(e: MouseEvent, s: Source) {
  const groups = data.groups
  ui.openMenu(e.clientX, e.clientY, [
    { label: t('item.markAllRead'), action: () => api.markAllRead('source', s.id).then(() => Promise.all([data.loadSources(), data.loadItems()])) },
    { label: t('feed.rename'), action: () => openRenameSourceModal(s.id, s.title) },
    { label: t('feed.refreshIcon'), action: () => reFetchFavicon(s.id) },
    { label: t('feed.changeIcon'), action: () => triggerCustomIconUpload(s.id) },
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
        requestConfirm({
          title: t('settings.sources.remove'),
          message: t('settings.sources.confirmRemove'),
          confirmText: t('settings.sources.remove'),
          danger: true,
          onConfirm: () => data.removeSource(s.id),
        })
      },
    },
  ])
}

function groupMenu(e: MouseEvent, gid: number) {
  ui.openMenu(e.clientX, e.clientY, [
    { label: t('item.markAllRead'), action: () => api.markAllRead('group', gid).then(() => Promise.all([data.loadSources(), data.loadItems()])) },
    { label: t('group.rename'), action: () => openGroupModal(gid, data.groups.find((g) => g.id === gid)?.name ?? '') },
    {
      label: t('group.delete'),
      danger: true,
      action: () => {
        requestConfirm({
          title: t('group.delete'),
          message: t('group.confirmDelete'),
          confirmText: t('group.delete'),
          danger: true,
          onConfirm: async () => {
            if (data.scope.type === 'group' && data.scope.id === gid) {
              await data.selectScope('all')
            }
            await api.deleteGroup(gid)
            await Promise.all([data.loadSources(), data.loadGroups(), data.loadItems()])
          },
        })
      },
    },
  ])
}

function openGroupModal(targetId: number | null, initialName: string) {
  modalMode = targetId === null ? 'createGroup' : 'renameGroup'
  groupModalTarget = targetId
  groupModalName.value = initialName
  groupModalTitle.value = targetId === null ? t('group.new') : t('group.rename')
  showGroupModal.value = true
}

function openRenameSourceModal(sourceId: number, initialTitle: string) {
  modalMode = 'renameSource'
  groupModalTarget = sourceId
  groupModalName.value = initialTitle
  groupModalTitle.value = t('feed.rename')
  showGroupModal.value = true
}

async function submitGroupModal() {
  const name = groupModalName.value.trim()
  if (!name) return
  if (modalMode === 'createGroup') {
    await api.createGroup(name)
    await Promise.all([data.loadGroups(), data.loadSources()])
  } else if (modalMode === 'renameGroup' && groupModalTarget !== null) {
    await api.renameGroup(groupModalTarget, name)
    await Promise.all([data.loadGroups(), data.loadSources()])
  } else if (modalMode === 'renameSource' && groupModalTarget !== null) {
    await api.renameSource(groupModalTarget, name)
    await data.loadSources()
  }
  showGroupModal.value = false
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
    <!-- macOS 窗口红绿灯控制按钮与标题栏 -->
    <div class="nav-header" data-tauri-drag-region>
      <TrafficLights />
      <div class="brand" data-tauri-drag-region>
        <span class="logo">
          <Icon name="rss" :size="13" color="#ffffff" />
        </span>
        <span class="app-name">ZReader</span>
      </div>
    </div>

    <!-- 导航主体 -->
    <div class="nav-body">
      <!-- 全部文章 -->
      <button
        class="nav-row all"
        :class="{ active: data.scope.type === 'all' }"
        @click="data.selectScope('all')"
      >
        <span class="row-icon">
          <Icon name="all" :size="16" />
        </span>
        <span class="row-title">{{ t('nav.all') }}</span>
        <span v-if="data.totalUnread" class="unread-badge">{{ data.totalUnread }}</span>
      </button>

      <div class="section-label" v-if="data.groups.length || ungroupedSources.length">
        <span>FEEDS</span>
      </div>

      <!-- 分组列表 -->
      <template v-for="g in data.groups" :key="g.id">
        <div class="nav-row group" :class="{ active: data.scope.type === 'group' && data.scope.id === g.id }">
          <button class="chevron" @click="toggleExpand(g.id)">
            <Icon :name="g.expanded ? 'chevron-down' : 'chevron-right'" :size="12" />
          </button>
          <button class="row-main" @click="data.selectScope('group', g.id)" @contextmenu.prevent="groupMenu($event, g.id)">
            <span class="row-icon folder-icon">
              <Icon name="folder" :size="15" />
            </span>
            <span class="row-title">{{ g.name }}</span>
            <span v-if="data.unreadOf({ type: 'group', id: g.id })" class="unread-badge">
              {{ data.unreadOf({ type: 'group', id: g.id }) }}
            </span>
          </button>
        </div>

        <!-- 分组内的订阅源 -->
        <template v-if="g.expanded">
          <button
            v-for="s in sourcesOf(g.id)"
            :key="s.id"
            class="nav-row source"
            :class="{ active: data.scope.type === 'source' && data.scope.id === s.id, error: s.errorCount > 0 }"
            :title="s.lastError ?? undefined"
            @click="data.selectScope('source', s.id)"
            @contextmenu.prevent="sourceMenu($event, s)"
          >
            <FeedIcon :source="s" :size="16" />
            <span class="row-title">{{ s.title }}</span>
            <span v-if="s.unread" class="unread-badge">{{ s.unread }}</span>
          </button>
        </template>
      </template>

      <div v-if="data.groups.length && ungroupedSources.length" class="divider"></div>

      <!-- 未分组的订阅源 -->
      <button
        v-for="s in ungroupedSources"
        :key="s.id"
          class="nav-row source ungrouped"
          :class="{ active: data.scope.type === 'source' && data.scope.id === s.id, error: s.errorCount > 0 }"
          :title="s.lastError ?? undefined"
          @click="data.selectScope('source', s.id)"
        @contextmenu.prevent="sourceMenu($event, s)"
      >
        <FeedIcon :source="s" :size="16" />
        <span class="row-title">{{ s.title }}</span>
        <span v-if="s.unread" class="unread-badge">{{ s.unread }}</span>
      </button>

      <!-- 上传自定义图标的隐藏文件输入框 -->
      <input
        ref="iconFileInput"
        type="file"
        accept="image/png,image/jpeg,image/x-icon,image/webp,image/gif"
        style="display: none;"
        @change="onIconFileSelected"
      />

      <!-- 空状态 -->
      <EmptyState
        v-if="!data.sources.length"
        icon="rss"
        :icon-size="24"
        :title="t('nav.noSources')"
        :description="t('nav.addHint')"
      />
    </div>

    <!-- macOS 毛玻璃底栏 -->
    <div class="nav-footer">
      <button class="f-icon-btn action-btn" :title="t('nav.addSource')" @click="$emit('add-source')">
        <Icon name="plus" :size="16" />
      </button>
      <button class="f-icon-btn action-btn" :title="t('group.new')" @click="openGroupModal(null, '')">
        <Icon name="folder-plus" :size="15" />
      </button>
      <button
        class="f-icon-btn action-btn"
        :class="{ spinning: data.fetching }"
        :title="t('nav.refresh')"
        @click="data.fetchAll()"
      >
        <Icon name="arrow-clockwise" :size="15" :class="{ spin: data.fetching }" />
      </button>
      <div class="spacer"></div>
      <button class="f-icon-btn action-btn" :title="t('nav.settings')" @click="$emit('open-settings')">
        <Icon name="gear" :size="15" />
      </button>
    </div>
  </nav>

  <!-- 分组操作的 macOS 工作表弹窗 -->
  <Modal v-if="showGroupModal" :title="groupModalTitle" @close="showGroupModal = false">
    <div class="form-field">
      <label class="form-label">{{ t('group.namePlaceholder') }}</label>
      <input
        v-model="groupModalName"
        class="apple-input"
        autofocus
        @keyup.enter="submitGroupModal"
      />
    </div>
    <template #footer>
      <button class="f-btn" @click="showGroupModal = false">{{ t('common.cancel') }}</button>
      <button class="f-btn primary" @click="submitGroupModal">{{ t('common.confirm') }}</button>
    </template>
  </Modal>

  <ConfirmModal
    v-if="confirmState.show"
    :title="confirmState.title"
    :message="confirmState.message"
    :confirm-text="confirmState.confirmText"
    :danger="confirmState.danger"
    :loading="confirmState.loading"
    @confirm="handleConfirm"
    @cancel="confirmState.show = false"
    @close="confirmState.show = false"
  />
</template>

<style scoped>
.side-nav {
  width: var(--nav-width);
  height: 100%;
  display: flex;
  flex-direction: column;
  background: var(--bg-sidebar);
  backdrop-filter: var(--glass-blur);
  -webkit-backdrop-filter: var(--glass-blur);
  border-right: 0.5px solid var(--border);
  flex-shrink: 0;
  user-select: none;
}

.nav-header {
  padding: 0.85rem 1rem 0.6rem;
  display: flex;
  flex-direction: column;
  gap: 0.65rem;
}

.brand {
  display: flex;
  align-items: center;
  gap: 0.55rem;
  padding: 0 0.2rem;
}

.logo {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 1.6rem;
  height: 1.6rem;
  border-radius: 7px;
  background: linear-gradient(135deg, #0a84ff 0%, #007aff 100%);
  box-shadow: 0 2px 5px rgba(0, 122, 255, 0.35);
}

.app-name {
  font-weight: 700;
  font-size: 0.95rem;
  letter-spacing: -0.025em;
  color: var(--text-primary);
}

.section-label {
  padding: 0.75rem 0.75rem 0.25rem;
  font-size: 0.68rem;
  font-weight: 700;
  letter-spacing: 0.06em;
  color: var(--text-tertiary);
}

.nav-body {
  flex: 1;
  overflow-y: auto;
  padding: 0 0.55rem 0.5rem;
}

.nav-row {
  display: flex;
  align-items: center;
  gap: 0.45rem;
  width: 100%;
  padding: 0.35rem 0.6rem;
  border-radius: 8px;
  text-align: left;
  color: var(--text-primary);
  transition: all 0.16s var(--ease);
  position: relative;
  font-size: 0.86rem;
  font-weight: 450;
}

.nav-row:hover:not(.active) {
  background: var(--bg-hover);
}

.nav-row.all.active,
.nav-row.group.active .row-main,
.nav-row.source.active {
  background: var(--accent);
  color: #ffffff;
  box-shadow: 0 1px 3px rgba(0, 122, 255, 0.25);
  font-weight: 600;
}

.nav-row.all.active .row-icon,
.nav-row.group.active .row-main .row-icon,
.nav-row.source.active .row-icon {
  color: #ffffff;
}

.nav-row.all.active .unread-badge,
.nav-row.group.active .row-main .unread-badge,
.nav-row.source.active .unread-badge {
  background: rgba(255, 255, 255, 0.24);
  color: #ffffff;
}

.nav-row.group {
  padding-left: 0.2rem;
}

.nav-row.source {
  padding-left: 1.7rem;
  font-size: 0.84rem;
  color: var(--text-secondary);
}

.nav-row.source.ungrouped {
  padding-left: 0.6rem;
}

.nav-row.error .row-title {
  color: var(--danger);
}

.nav-row.error.active .row-title {
  color: #ffffff;
}

.chevron {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 1.25rem;
  height: 1.6rem;
  color: var(--text-tertiary);
  border-radius: 4px;
  transition: color 0.15s ease;
}

.chevron:hover {
  color: var(--text-primary);
}

.row-main {
  display: flex;
  align-items: center;
  gap: 0.45rem;
  flex: 1;
  min-width: 0;
  padding: 0.35rem 0.5rem;
  border-radius: 7px;
  margin: 0 -0.15rem;
  text-align: left;
}

.row-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 1.2rem;
  flex-shrink: 0;
  color: var(--accent);
}

.row-title {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.divider {
  height: 0.5px;
  background: var(--border);
  margin: 0.5rem 0.6rem;
}

.empty-hint {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 2.2rem 1rem;
  color: var(--text-tertiary);
  font-size: 0.82rem;
  text-align: center;
  gap: 0.45rem;
}

.empty-hint .sub {
  font-size: 0.74rem;
  color: var(--text-quaternary);
}

.nav-footer {
  display: flex;
  align-items: center;
  gap: 0.2rem;
  padding: 0.45rem 0.75rem calc(0.55rem);
  border-top: 0.5px solid var(--border);
  background: var(--bg-sidebar);
}

.action-btn {
  color: var(--text-secondary);
  border-radius: 7px;
}

.action-btn:hover {
  color: var(--accent);
  background: var(--bg-hover-strong);
}

.spacer {
  flex: 1;
}

.input-label {
  display: block;
  font-size: 0.8rem;
  font-weight: 500;
  color: var(--text-secondary);
  margin-bottom: 0.35rem;
}

.apple-input {
  width: 100%;
}
</style>
