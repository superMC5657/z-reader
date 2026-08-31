<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDataStore } from '../stores/data'
import { useAppStore } from '../stores/app'
import * as api from '../lib/tauri'
import Modal from './ui/Modal.vue'
import Icon from './ui/Icon.vue'
import FeedIcon from './ui/FeedIcon.vue'
import AppleSelect from './ui/AppleSelect.vue'
import Switch from './ui/Switch.vue'
import { LOCALES } from '../i18n'

const { t } = useI18n()
const data = useDataStore()
const app = useAppStore()

const emit = defineEmits<{ close: [] }>()

const tab = ref<'sources' | 'app' | 'shortcuts' | 'data' | 'about'>('sources')
const tabs = computed(() => [
  { value: 'sources', label: t('settings.tabs.sources'), icon: 'sources' },
  { value: 'app', label: t('settings.tabs.app'), icon: 'app' },
  { value: 'shortcuts', label: t('settings.tabs.shortcuts'), icon: 'keyboard' },
  { value: 'data', label: t('settings.tabs.data'), icon: 'data' },
  { value: 'about', label: t('settings.tabs.about'), icon: 'info' },
])

const recordingAction = ref<string | null>(null)

function startRecording(actionKey: string) {
  recordingAction.value = actionKey
  window.addEventListener('keydown', onRecordKeydown, { capture: true, once: true })
}

function onRecordKeydown(e: KeyboardEvent) {
  e.preventDefault()
  e.stopPropagation()
  if (!recordingAction.value) return

  let keyName = e.key
  if (keyName === ' ') keyName = 'Space'
  else if (keyName.length === 1) keyName = keyName.toLowerCase()

  app.setShortcut(recordingAction.value, keyName)
  recordingAction.value = null
}

function formatKeyDisplay(k?: string): string {
  if (!k) return '—'
  if (k === 'Escape') return '⎋ Esc'
  if (k === 'Space' || k === ' ') return '␣ Space'
  if (k === 'ArrowUp') return '↑'
  if (k === 'ArrowDown') return '↓'
  if (k === 'ArrowLeft') return '←'
  if (k === 'ArrowRight') return '→'
  if (k === 'Enter') return '↵ Enter'
  return k.toUpperCase()
}

const shortcutGroups = computed(() => [
  {
    title: t('settings.shortcuts.navSection'),
    items: [
      { key: 'nextArticle', label: t('settings.shortcuts.nextArticle') },
      { key: 'prevArticle', label: t('settings.shortcuts.prevArticle') },
      { key: 'closeArticle', label: t('settings.shortcuts.closeArticle') },
    ],
  },
  {
    title: t('settings.shortcuts.actionSection'),
    items: [
      { key: 'toggleRead', label: t('settings.shortcuts.toggleRead') },
      { key: 'toggleStar', label: t('settings.shortcuts.toggleStar') },
      { key: 'fetchFull', label: t('settings.shortcuts.fetchFull') },
      { key: 'openInBrowser', label: t('settings.shortcuts.openInBrowser') },
    ],
  },
  {
    title: t('settings.shortcuts.globalSection'),
    items: [
      { key: 'refresh', label: t('settings.shortcuts.refresh') },
      { key: 'addSource', label: t('settings.shortcuts.addSource') },
      { key: 'toggleSidebar', label: t('settings.shortcuts.toggleSidebar') },
    ],
  },
])

const themeOptions = computed(() => [
  { value: 'system', label: t('settings.app.themeSystem'), icon: 'display' },
  { value: 'light', label: t('settings.app.themeLight'), icon: 'sun' },
  { value: 'dark', label: t('settings.app.themeDark'), icon: 'moon' },
])

const viewOptions = computed(() => [
  { value: 'cards', label: t('toolbar.views.cards'), icon: 'view-cards' },
  { value: 'list', label: t('toolbar.views.list'), icon: 'view-list' },
  { value: 'magazine', label: t('toolbar.views.magazine'), icon: 'view-magazine' },
  { value: 'compact', label: t('toolbar.views.compact'), icon: 'view-compact' },
])

const readerModeOptions = computed(() => [
  { value: 'split', label: t('settings.app.readerModeSplit'), icon: 'layout-split' },
  { value: 'focus', label: t('settings.app.readerModeFocus'), icon: 'focus' },
])

const localeOptions = computed(() =>
  LOCALES.map((l) => ({ value: l.value, label: l.label, icon: 'globe' }))
)

const groupOptions = computed(() => [
  { value: '', label: t('settings.sources.ungrouped'), icon: 'rss' },
  ...data.groups.map((g) => ({ value: String(g.id), label: g.name, icon: 'folder' })),
])

function onSourceGroupChange(id: number, val: string) {
  const gid = val === '' ? null : Number(val)
  api
    .setSourceGroup(id, gid)
    .then(() => Promise.all([data.loadSources(), data.loadGroups()]))
}

const dataMsg = ref('')
const opmlInput = ref<HTMLInputElement | null>(null)

function removeSource(id: number) {
  if (confirm(t('settings.sources.confirmRemove'))) {
    data.removeSource(id)
  }
}

async function importOpml(e: Event) {
  const file = (e.target as HTMLInputElement).files?.[0]
  if (!file) return
  const text = await file.text()
  try {
    const r = await api.importOpml(text)
    dataMsg.value = t('settings.data.imported', {
      sources: r.sourcesAdded,
      groups: r.groupsAdded,
      existing: r.sourcesExisting,
    })
    await Promise.all([data.loadSources(), data.loadGroups(), data.loadItems()])
  } catch (err) {
    dataMsg.value = String(err)
  }
  const target = e.target as HTMLInputElement
  target.value = ''
}

async function exportOpml() {
  const xml = await api.exportOpml()
  const blob = new Blob([xml], { type: 'text/xml' })
  const a = document.createElement('a')
  a.href = URL.createObjectURL(blob)
  a.download = 'z-reader-subscriptions.opml'
  a.click()
  URL.revokeObjectURL(a.href)
  dataMsg.value = t('settings.data.exported')
}

function toggleViewConfig(bit: number, val: boolean) {
  if (val) {
    app.patch({ viewConfigs: app.s.viewConfigs | bit })
  } else {
    app.patch({ viewConfigs: app.s.viewConfigs & ~bit })
  }
}

function adjustFontSize(delta: number) {
  const cur = app.s.fontSize || 16
  const next = Math.min(22, Math.max(12, cur + delta))
  app.patch({ fontSize: next })
}
</script>

<template>
  <Modal :title="t('settings.title')" wide @close="emit('close')">
    <!-- Apple macOS Segmented Settings Tabs -->
    <div class="tabs-container">
      <div class="segmented settings-segmented">
        <button
          v-for="tab_ in tabs"
          :key="tab_.value"
          class="seg settings-seg"
          :class="{ active: tab === tab_.value }"
          @click="tab = tab_.value as typeof tab"
        >
          <Icon :name="tab_.icon" :size="14" />
          <span>{{ tab_.label }}</span>
        </button>
      </div>
    </div>

    <!-- Tab: Sources -->
    <div v-if="tab === 'sources'" class="tab-body">
      <div v-if="data.sources.length" class="grouped-inset-box">
        <div v-for="s in data.sources" :key="s.id" class="grouped-inset-row">
          <div class="source-info">
            <div class="source-title-row">
              <FeedIcon :source="s" :size="16" />
              <span class="source-title">{{ s.title }}</span>
            </div>
            <span class="source-url">{{ s.url }}</span>
          </div>
          <div class="source-actions">
            <AppleSelect
              compact
              :model-value="s.groupId !== null ? String(s.groupId) : ''"
              :options="groupOptions"
              @update:model-value="onSourceGroupChange(s.id, $event)"
            />
            <button class="f-icon-btn remove-btn" :title="t('settings.sources.remove')" @click="removeSource(s.id)">
              <Icon name="trash" :size="15" color="var(--danger)" />
            </button>
          </div>
        </div>
      </div>
      <div v-else class="empty-sources">
        <Icon name="rss" :size="32" color="var(--text-quaternary)" />
        <p>{{ t('nav.noSources') }}</p>
      </div>
    </div>

    <!-- Tab: App Settings (macOS Inset Grouped) -->
    <div v-else-if="tab === 'app'" class="tab-body">
      <div class="grouped-inset-box">
        <!-- Theme -->
        <div class="grouped-inset-row">
          <div class="label-box">
            <span class="label-title">{{ t('settings.app.theme') }}</span>
          </div>
          <AppleSelect
            :model-value="app.s.theme"
            :options="themeOptions"
            @update:model-value="app.patch({ theme: $event })"
          />
        </div>

        <!-- Default View -->
        <div class="grouped-inset-row">
          <div class="label-box">
            <span class="label-title">{{ t('settings.app.view') }}</span>
          </div>
          <AppleSelect
            :model-value="app.s.view"
            :options="viewOptions"
            @update:model-value="app.patch({ view: $event })"
          />
        </div>

        <!-- Reader Mode -->
        <div class="grouped-inset-row">
          <div class="label-box">
            <span class="label-title">{{ t('settings.app.readerMode') }}</span>
          </div>
          <AppleSelect
            :model-value="app.s.readerMode || 'split'"
            :options="readerModeOptions"
            @update:model-value="app.patch({ readerMode: $event })"
          />
        </div>

        <!-- Language -->
        <div class="grouped-inset-row">
          <div class="label-box">
            <span class="label-title">{{ t('settings.app.language') }}</span>
          </div>
          <AppleSelect
            :model-value="app.s.locale"
            :options="localeOptions"
            @update:model-value="app.patch({ locale: $event })"
          />
        </div>

        <!-- Font Size Slider -->
        <div class="grouped-inset-row">
          <div class="label-box">
            <span class="label-title">{{ t('settings.app.fontSize') }}</span>
          </div>
          <div class="slider-row">
            <button
              class="f-icon-btn stepper-btn"
              title="A-"
              :disabled="app.s.fontSize <= 12"
              @click="adjustFontSize(-1)"
            >
              <span class="stepper-label small">A-</span>
            </button>
            <input
              type="range"
              min="12"
              max="22"
              step="1"
              :value="app.s.fontSize"
              @input="app.patch({ fontSize: Number(($event.target as HTMLInputElement).value) })"
            />
            <button
              class="f-icon-btn stepper-btn"
              title="A+"
              :disabled="app.s.fontSize >= 22"
              @click="adjustFontSize(1)"
            >
              <span class="stepper-label large">A+</span>
            </button>
            <span class="value-badge">{{ app.s.fontSize }}px</span>
          </div>
        </div>

        <!-- Background Fetch Interval -->
        <div class="grouped-inset-row">
          <div class="label-box">
            <span class="label-title">{{ t('settings.app.fetchInterval') }}</span>
          </div>
          <input
            type="number"
            min="1"
            max="1440"
            style="width: 6.5rem; text-align: center"
            :value="app.s.fetchInterval"
            @change="app.patch({ fetchInterval: Math.max(1, Number(($event.target as HTMLInputElement).value) || 30) })"
          />
        </div>
      </div>

      <!-- View Configuration Switches -->
      <div class="grouped-inset-box">
        <div class="grouped-inset-row">
          <div class="label-box">
            <span class="label-title">{{ t('settings.app.showCover') }}</span>
          </div>
          <Switch
            :model-value="app.showCover"
            @update:model-value="toggleViewConfig(1, $event)"
          />
        </div>

        <div class="grouped-inset-row">
          <div class="label-box">
            <span class="label-title">{{ t('settings.app.showSnippet') }}</span>
          </div>
          <Switch
            :model-value="app.showSnippet"
            @update:model-value="toggleViewConfig(2, $event)"
          />
        </div>

        <div class="grouped-inset-row">
          <div class="label-box">
            <span class="label-title">{{ t('settings.app.fadeRead') }}</span>
          </div>
          <Switch
            :model-value="app.fadeRead"
            @update:model-value="toggleViewConfig(4, $event)"
          />
        </div>
      </div>
    </div>

    <!-- Tab: Shortcuts -->
    <div v-else-if="tab === 'shortcuts'" class="tab-body">
      <div class="shortcuts-header-bar">
        <p class="shortcuts-hint">{{ t('settings.shortcuts.hint') }}</p>
        <button class="f-btn compact-btn" @click="app.resetShortcuts()">
          {{ t('settings.shortcuts.reset') }}
        </button>
      </div>

      <div v-for="group in shortcutGroups" :key="group.title" class="shortcut-group-wrapper">
        <div class="shortcut-group-title">{{ group.title }}</div>
        <div class="grouped-inset-box">
          <div v-for="item in group.items" :key="item.key" class="grouped-inset-row">
            <span class="label-title">{{ item.label }}</span>
            <button
              class="keycap-btn"
              :class="{ recording: recordingAction === item.key }"
              @click="startRecording(item.key)"
            >
              <span v-if="recordingAction === item.key" class="recording-text">
                {{ t('settings.shortcuts.pressKey') }}
              </span>
              <kbd v-else class="apple-keycap">
                {{ formatKeyDisplay(app.shortcuts[item.key]) }}
              </kbd>
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- Tab: Data (OPML) -->
    <div v-else-if="tab === 'data'" class="tab-body">
      <div class="grouped-inset-box">
        <div class="grouped-inset-row">
          <div class="label-box">
            <span class="label-title">{{ t('settings.data.importOpml') }}</span>
            <span class="label-desc">从 OPML 文件恢复订阅与分组</span>
          </div>
          <button class="f-btn" @click="opmlInput?.click()">
            <Icon name="import" :size="14" />
            {{ t('settings.data.importOpml') }}
          </button>
          <input ref="opmlInput" type="file" accept=".opml,.xml" hidden @change="importOpml" />
        </div>

        <div class="grouped-inset-row">
          <div class="label-box">
            <span class="label-title">{{ t('settings.data.exportOpml') }}</span>
            <span class="label-desc">将全部订阅源导出为标准 OPML 文件</span>
          </div>
          <button class="f-btn" @click="exportOpml">
            <Icon name="export" :size="14" />
            {{ t('settings.data.exportOpml') }}
          </button>
        </div>
      </div>

      <div v-if="dataMsg" class="info-banner">
        <Icon name="checkmark" :size="14" color="var(--success)" />
        <span>{{ dataMsg }}</span>
      </div>
    </div>

    <!-- Tab: About -->
    <div v-else class="tab-body">
      <div class="about-card">
        <div class="about-logo">
          <Icon name="rss" :size="28" color="#ffffff" />
        </div>
        <h3 class="about-title">ZReader</h3>
        <p class="about-ver">{{ t('settings.about.version') }} {{ app.s.version }}</p>
        <p class="about-desc">{{ t('settings.about.desc') }}</p>
      </div>
    </div>

    <template #footer>
      <button class="f-btn primary" @click="emit('close')">{{ t('common.confirm') }}</button>
    </template>
  </Modal>
</template>

<style scoped>
.tabs-container {
  display: flex;
  justify-content: center;
  margin-bottom: 1.1rem;
}

.settings-segmented {
  padding: 3px;
}

.settings-seg {
  padding: 0.35rem 1.1rem;
  font-size: 0.84rem;
}

.tab-body {
  min-height: 18rem;
  max-height: 24rem;
  overflow-y: auto;
}

.source-info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 0.18rem;
}

.source-title-row {
  display: flex;
  align-items: center;
  gap: 0.45rem;
  min-width: 0;
}

.settings-favicon {
  width: 1rem;
  height: 1rem;
  border-radius: 3px;
  object-fit: cover;
  flex-shrink: 0;
}

.source-title {
  font-weight: 600;
  font-size: 0.88rem;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.source-url {
  font-size: 0.72rem;
  color: var(--text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.source-actions {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.group-select {
  font-size: 0.8rem;
  padding: 0.28rem 1.8rem 0.28rem 0.65rem;
  max-width: 10rem;
}

.remove-btn {
  border-radius: 6px;
}

.remove-btn:hover {
  background: var(--danger-tint);
}

.empty-sources {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 14rem;
  color: var(--text-tertiary);
  gap: 0.6rem;
  font-size: 0.9rem;
}

.slider-row {
  display: flex;
  align-items: center;
  gap: 0.45rem;
}

.stepper-btn {
  width: 1.65rem;
  height: 1.65rem;
  border-radius: 6px;
  background: var(--bg-track);
  color: var(--text-secondary);
}

.stepper-btn:hover:not(:disabled) {
  background: var(--bg-hover-strong);
  color: var(--text-primary);
}

.stepper-label {
  font-weight: 600;
  line-height: 1;
  user-select: none;
}

.stepper-label.small {
  font-size: 0.7rem;
}

.stepper-label.large {
  font-size: 0.82rem;
}

.value-badge {
  font-size: 0.78rem;
  font-weight: 600;
  color: var(--text-secondary);
  background: var(--bg-track);
  padding: 0.1rem 0.45rem;
  border-radius: var(--radius-pill);
  font-variant-numeric: tabular-nums;
}

.info-banner {
  margin-top: 0.8rem;
  padding: 0.75rem 1rem;
  border-radius: 10px;
  background: var(--bg-card);
  border: 0.5px solid var(--border);
  font-size: 0.82rem;
  color: var(--text-secondary);
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.about-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 2.5rem 1.5rem;
  text-align: center;
}

.about-logo {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 4rem;
  height: 4rem;
  border-radius: 16px;
  background: linear-gradient(135deg, #0a84ff 0%, #007aff 100%);
  box-shadow: 0 4px 14px rgba(0, 122, 255, 0.4);
  margin-bottom: 0.85rem;
}

.about-title {
  font-size: 1.25rem;
  font-weight: 700;
  letter-spacing: -0.025em;
}

.about-ver {
  font-size: 0.82rem;
  color: var(--text-tertiary);
  margin: 0.25rem 0 0.75rem;
}

.about-desc {
  font-size: 0.84rem;
  color: var(--text-secondary);
  max-width: 22rem;
  line-height: 1.5;
}

/* Shortcuts Tab Styles */
.shortcuts-header-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  margin-bottom: 0.85rem;
  padding: 0 0.2rem;
}

.shortcuts-hint {
  font-size: 0.8rem;
  color: var(--text-tertiary);
  margin: 0;
}

.compact-btn {
  font-size: 0.76rem;
  padding: 0.24rem 0.65rem;
  border-radius: 6px;
  flex-shrink: 0;
}

.shortcut-group-wrapper {
  margin-bottom: 1.1rem;
}

.shortcut-group-title {
  font-size: 0.72rem;
  font-weight: 700;
  letter-spacing: 0.04em;
  color: var(--text-tertiary);
  text-transform: uppercase;
  margin-bottom: 0.4rem;
  padding-left: 0.4rem;
}

.keycap-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: none;
  cursor: pointer;
  padding: 0;
  outline: none;
}

.apple-keycap {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 2.2rem;
  height: 1.7rem;
  padding: 0 0.6rem;
  font-family: -apple-system, BlinkMacSystemFont, 'SF Pro Text', system-ui, sans-serif;
  font-size: 0.82rem;
  font-weight: 600;
  color: var(--text-primary);
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-bottom: 2px solid var(--border-strong);
  border-radius: 6px;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.06);
  transition: all 0.15s var(--ease);
}

.keycap-btn:hover .apple-keycap {
  background: var(--bg-hover-strong);
  border-color: var(--accent);
  color: var(--accent);
  transform: translateY(-1px);
}

.keycap-btn:active .apple-keycap {
  transform: translateY(1px);
  border-bottom-width: 1px;
}

.keycap-btn.recording .recording-text {
  font-size: 0.78rem;
  font-weight: 600;
  color: var(--accent);
  background: var(--accent-tint);
  padding: 0.24rem 0.7rem;
  border-radius: 6px;
  border: 1.5px dashed var(--accent);
  animation: pulseRecord 1.2s infinite ease-in-out;
}

@keyframes pulseRecord {
  0%, 100% {
    opacity: 1;
    transform: scale(1);
  }
  50% {
    opacity: 0.65;
    transform: scale(0.97);
  }
}
</style>
