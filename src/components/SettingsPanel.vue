<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDataStore } from '../stores/data'
import { useAppStore } from '../stores/app'
import * as api from '../lib/tauri'
import type { Item, Rule, RuleBackfillResult } from '../types'
import Modal from './ui/Modal.vue'
import Icon from './ui/Icon.vue'
import FeedIcon from './ui/FeedIcon.vue'
import AppleSelect from './ui/AppleSelect.vue'
import Switch from './ui/Switch.vue'
import RuleEditDialog from './RuleEditDialog.vue'
import { LOCALES } from '../i18n'
import {
  updateState,
  checkForUpdates,
  startDownloadAndInstall,
  restartApp,
} from '../lib/updater'

const { t } = useI18n()
const data = useDataStore()
const app = useAppStore()

const emit = defineEmits<{ close: [] }>()

const tab = ref<'sources' | 'general' | 'rules' | 'app' | 'shortcuts' | 'data' | 'about'>('sources')
const tabs = computed(() => [
  { value: 'sources', label: t('settings.tabs.sources'), icon: 'sources' },
  { value: 'general', label: t('settings.tabs.general'), icon: 'gear' },
  { value: 'rules', label: t('settings.tabs.rules'), icon: 'funnel' },
  { value: 'app', label: t('settings.tabs.app'), icon: 'app' },
  { value: 'shortcuts', label: t('settings.tabs.shortcuts'), icon: 'keyboard' },
  { value: 'data', label: t('settings.tabs.data'), icon: 'data' },
  { value: 'about', label: t('settings.tabs.about'), icon: 'info' },
])

// ---------- General tab: proxy / notification / tray / storage ----------

const proxyModeOptions = computed(() => [
  { value: 'system', label: t('settings.general.proxySystem'), icon: 'display' },
  { value: 'none', label: t('settings.general.proxyNone'), icon: 'close' },
  { value: 'manual', label: t('settings.general.proxyManual'), icon: 'globe' },
])

const retentionOptions = computed(() => [
  { value: 0, label: t('settings.general.retentionNever'), icon: 'data' },
  { value: 7, label: t('settings.general.retentionDays', { n: 7 }), icon: 'data' },
  { value: 30, label: t('settings.general.retentionDays', { n: 30 }), icon: 'data' },
  { value: 90, label: t('settings.general.retentionDays', { n: 90 }), icon: 'data' },
  { value: 180, label: t('settings.general.retentionDays', { n: 180 }), icon: 'data' },
])

const maxPerSourceOptions = computed(() => [
  { value: 0, label: t('settings.general.maxUnlimited'), icon: 'data' },
  { value: 100, label: t('settings.general.maxCount', { n: 100 }), icon: 'data' },
  { value: 200, label: t('settings.general.maxCount', { n: 200 }), icon: 'data' },
  { value: 500, label: t('settings.general.maxCount', { n: 500 }), icon: 'data' },
  { value: 1000, label: t('settings.general.maxCount', { n: 1000 }), icon: 'data' },
])

const proxyTesting = ref(false)
const proxyResult = ref<string>('')

async function testProxy() {
  proxyTesting.value = true
  proxyResult.value = ''
  try {
    const ms = await api.testProxy(app.s)
    proxyResult.value = t('settings.general.proxyOk', { ms })
  } catch (err) {
    proxyResult.value = t('settings.general.proxyFail', { err: String(err) })
  } finally {
    proxyTesting.value = false
  }
}

const stats = ref<{ articles: number; unread: number; dbSize: number } | null>(null)
const cleaningUp = ref(false)
const cleanupMsg = ref('')

function formatBytes(n: number): string {
  if (n >= 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`
  if (n >= 1024) return `${(n / 1024).toFixed(0)} KB`
  return `${n} B`
}

async function loadStats() {
  try {
    stats.value = await api.getStats()
  } catch {
    stats.value = null
  }
}

async function cleanupNow() {
  cleaningUp.value = true
  cleanupMsg.value = ''
  try {
    const deleted = await api.cleanupNow()
    cleanupMsg.value = t('settings.general.cleanupDone', { n: deleted })
    await loadStats()
    await data.loadSources()
  } catch (err) {
    cleanupMsg.value = String(err)
  } finally {
    cleaningUp.value = false
  }
}

watch(tab, (v) => {
  if (v === 'general') loadStats()
  if (v === 'rules') loadRulesTab()
})

// ---------- Rules tab ----------

const rules = ref<Rule[]>([])
const rulesLoading = ref(false)
const showRuleEditor = ref(false)
const editingRule = ref<Rule | null>(null)
const backfillMsg = ref('')
const hiddenItems = ref<Item[]>([])

async function loadRulesTab() {
  rulesLoading.value = true
  try {
    const [rs, hidden] = await Promise.all([
      api.getRules(),
      api.getItems({ filter: 3, limit: 100 }),
    ])
    rules.value = rs
    hiddenItems.value = hidden
  } finally {
    rulesLoading.value = false
  }
}

function openRuleEditor(rule: Rule | null) {
  editingRule.value = rule
  showRuleEditor.value = true
}

function onRuleSaved() {
  showRuleEditor.value = false
  loadRulesTab()
}

async function toggleRule(rule: Rule, enabled: boolean) {
  await api.updateRule(rule.id, {
    name: rule.name,
    pattern: rule.pattern,
    targetField: rule.targetField,
    actionType: rule.actionType,
    isCaseSensitive: rule.isCaseSensitive,
    isEnabled: enabled,
    sourceScope: rule.sourceScope,
  })
  rule.isEnabled = enabled
}

async function removeRule(id: number) {
  if (confirm(t('settings.rules.confirmDelete'))) {
    await api.deleteRule(id)
    await loadRulesTab()
  }
}

async function unhideItem(id: number) {
  await api.setItemHidden(id, false)
  hiddenItems.value = hiddenItems.value.filter((i) => i.id !== id)
}

async function applyBackfill() {
  backfillMsg.value = ''
  try {
    const r: RuleBackfillResult = await api.applyRulesBackfill()
    backfillMsg.value = t('settings.rules.backfillDone', {
      read: r.markedRead,
      starred: r.starred,
      hidden: r.hidden,
    })
    await loadRulesTab()
  } catch (err) {
    backfillMsg.value = String(err)
  }
}

function scopeLabel(scope: string): string {
  if (scope === 'all') return t('settings.rules.scopeAll')
  const [, idStr] = scope.split(':')
  const id = Number(idStr)
  if (scope.startsWith('source:')) {
    return `${t('settings.rules.scopeSourcePrefix')}${data.sourceById(id)?.title ?? id}`
  }
  const group = data.groups.find((g) => g.id === id)
  return `${t('settings.rules.scopeGroupPrefix')}${group?.name ?? id}`
}

// ---------- End Phase 2 sections ----------

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
  { value: 'magazine', label: t('toolbar.views.magazine'), icon: 'view-magazine' },
  { value: 'list', label: t('toolbar.views.list'), icon: 'view-list' },
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

// ---------- Full backup & restore ----------

const backingUp = ref(false)
const restoreConfirmVisible = ref(false)

async function doExportBackup() {
  backingUp.value = true
  dataMsg.value = ''
  try {
    const path = await api.exportBackup()
    dataMsg.value = path
      ? t('settings.data.backupDone', { path })
      : t('settings.data.backupCancelled')
  } catch (err) {
    dataMsg.value = String(err)
  } finally {
    backingUp.value = false
  }
}

async function doImportBackup() {
  restoreConfirmVisible.value = false
  backingUp.value = true
  dataMsg.value = ''
  try {
    const path = await api.importBackup()
    if (path) {
      // A successful restore swaps the database; relaunch for a clean reload.
      await restartApp()
    }
  } catch (err) {
    dataMsg.value = String(err)
  } finally {
    backingUp.value = false
  }
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

    <!-- Tab: General (network / notification / tray / storage) -->
    <div v-else-if="tab === 'general'" class="tab-body">
      <div class="grouped-inset-box">
        <div class="grouped-inset-row">
          <div class="label-box">
            <span class="label-title">{{ t('settings.general.proxyMode') }}</span>
            <span class="label-desc">{{ t('settings.general.proxyModeDesc') }}</span>
          </div>
          <AppleSelect
            :model-value="app.s.proxyMode"
            :options="proxyModeOptions"
            @update:model-value="app.patch({ proxyMode: $event })"
          />
        </div>

        <template v-if="app.s.proxyMode === 'manual'">
          <div class="grouped-inset-row">
            <div class="label-box">
              <span class="label-title">{{ t('settings.general.proxyUrl') }}</span>
              <span class="label-desc">http://127.0.0.1:7890 · socks5://127.0.0.1:1080</span>
            </div>
            <input
              class="apple-text-input"
              style="width: 13rem"
              placeholder="http://127.0.0.1:7890"
              :value="app.s.proxyUrl"
              @change="app.patch({ proxyUrl: ($event.target as HTMLInputElement).value.trim() })"
            />
          </div>
          <div class="grouped-inset-row">
            <div class="label-box">
              <span class="label-title">{{ t('settings.general.proxyAuth') }}</span>
            </div>
            <div class="auth-inputs">
              <input
                class="apple-text-input"
                style="width: 6.5rem"
                :placeholder="t('settings.general.proxyUser')"
                :value="app.s.proxyUsername"
                @change="app.patch({ proxyUsername: ($event.target as HTMLInputElement).value })"
              />
              <input
                class="apple-text-input"
                style="width: 6.5rem"
                type="password"
                :placeholder="t('settings.general.proxyPassword')"
                :value="app.s.proxyPassword"
                @change="app.patch({ proxyPassword: ($event.target as HTMLInputElement).value })"
              />
            </div>
          </div>
        </template>

        <div class="grouped-inset-row">
          <div class="label-box">
            <span class="label-title">{{ t('settings.general.testProxy') }}</span>
            <span v-if="proxyResult" class="label-desc">{{ proxyResult }}</span>
          </div>
          <button class="f-btn" :disabled="proxyTesting" @click="testProxy">
            <Icon name="globe" :size="14" />
            {{ proxyTesting ? t('settings.general.testing') : t('settings.general.testProxy') }}
          </button>
        </div>
      </div>

      <div class="grouped-inset-box">
        <div class="grouped-inset-row">
          <div class="label-box">
            <span class="label-title">{{ t('settings.general.notifyOnNew') }}</span>
            <span class="label-desc">{{ t('settings.general.notifyOnNewDesc') }}</span>
          </div>
          <Switch
            :model-value="app.s.notifyOnNew"
            @update:model-value="app.patch({ notifyOnNew: $event })"
          />
        </div>

        <div class="grouped-inset-row">
          <div class="label-box">
            <span class="label-title">{{ t('settings.general.closeToTray') }}</span>
            <span class="label-desc">{{ t('settings.general.closeToTrayDesc') }}</span>
          </div>
          <Switch
            :model-value="app.s.closeToTray"
            @update:model-value="app.patch({ closeToTray: $event })"
          />
        </div>
      </div>

      <div class="grouped-inset-box">
        <div class="grouped-inset-row">
          <div class="label-box">
            <span class="label-title">{{ t('settings.general.retentionLabel') }}</span>
            <span class="label-desc">{{ t('settings.general.retentionDesc') }}</span>
          </div>
          <AppleSelect
            :model-value="app.s.retentionDays"
            :options="retentionOptions"
            @update:model-value="app.patch({ retentionDays: $event })"
          />
        </div>

        <div class="grouped-inset-row">
          <div class="label-box">
            <span class="label-title">{{ t('settings.general.maxPerSource') }}</span>
            <span class="label-desc">{{ t('settings.general.maxDesc') }}</span>
          </div>
          <AppleSelect
            :model-value="app.s.maxItemsPerSource"
            :options="maxPerSourceOptions"
            @update:model-value="app.patch({ maxItemsPerSource: $event })"
          />
        </div>

        <div class="grouped-inset-row">
          <div class="label-box">
            <span class="label-title">{{ t('settings.general.cleanupNow') }}</span>
            <span v-if="stats" class="label-desc">
              {{ t('settings.general.statsLine', {
                articles: stats.articles,
                unread: stats.unread,
                size: formatBytes(stats.dbSize),
              }) }}
            </span>
          </div>
          <button class="f-btn" :disabled="cleaningUp" @click="cleanupNow">
            <Icon name="refresh" :size="14" />
            {{ cleaningUp ? t('common.loading') : t('settings.general.cleanupNow') }}
          </button>
        </div>
      </div>

      <div v-if="cleanupMsg" class="info-banner">
        <Icon name="checkmark" :size="14" color="var(--success)" />
        <span>{{ cleanupMsg }}</span>
      </div>
    </div>

    <!-- Tab: Regex Rules -->
    <div v-else-if="tab === 'rules'" class="tab-body">
      <div class="rules-toolbar">
        <p class="rules-hint">{{ t('settings.rules.hint') }}</p>
        <div class="rules-toolbar-actions">
          <button class="f-btn compact-btn" @click="applyBackfill">
            <Icon name="sparkles" :size="13" />
            {{ t('settings.rules.applyBackfill') }}
          </button>
          <button class="f-btn primary compact-btn" @click="openRuleEditor(null)">
            <Icon name="plus" :size="13" />
            {{ t('settings.rules.new') }}
          </button>
        </div>
      </div>

      <div v-if="rules.length" class="grouped-inset-box">
        <div v-for="r in rules" :key="r.id" class="grouped-inset-row">
          <div class="rule-info">
            <div class="rule-name-row">
              <span class="rule-name">{{ r.name }}</span>
              <span class="rule-badge action">{{ t(`settings.rules.action_${r.actionType}`) }}</span>
              <span class="rule-badge scope">{{ scopeLabel(r.sourceScope) }}</span>
            </div>
            <code class="rule-pattern">{{ r.pattern }}</code>
          </div>
          <div class="source-actions">
            <Switch :model-value="r.isEnabled" @update:model-value="toggleRule(r, $event)" />
            <button class="f-icon-btn" :title="t('feed.rename')" @click="openRuleEditor(r)">
              <Icon name="pencil" :size="14" />
            </button>
            <button class="f-icon-btn remove-btn" :title="t('settings.rules.delete')" @click="removeRule(r.id)">
              <Icon name="trash" :size="15" color="var(--danger)" />
            </button>
          </div>
        </div>
      </div>
      <div v-else class="empty-sources">
        <Icon name="funnel" :size="32" color="var(--text-quaternary)" />
        <p>{{ t('settings.rules.empty') }}</p>
      </div>

      <div v-if="backfillMsg" class="info-banner">
        <Icon name="checkmark" :size="14" color="var(--success)" />
        <span>{{ backfillMsg }}</span>
      </div>

      <div v-if="hiddenItems.length" class="hidden-section">
        <div class="shortcut-group-title">{{ t('settings.rules.hiddenSection', { n: hiddenItems.length }) }}</div>
        <div class="grouped-inset-box">
          <div v-for="h in hiddenItems" :key="h.id" class="grouped-inset-row">
            <div class="rule-info">
              <span class="rule-name">{{ h.title }}</span>
            </div>
            <button class="f-btn compact-btn" @click="unhideItem(h.id)">
              {{ t('settings.rules.unhide') }}
            </button>
          </div>
        </div>
      </div>

      <RuleEditDialog
        v-if="showRuleEditor"
        :rule="editingRule"
        :sources="data.sources"
        :groups="data.groups"
        @close="showRuleEditor = false"
        @saved="onRuleSaved"
      />
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
            <span class="label-desc">{{ t('settings.data.importOpmlDesc') }}</span>
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
            <span class="label-desc">{{ t('settings.data.exportOpmlDesc') }}</span>
          </div>
          <button class="f-btn" @click="exportOpml">
            <Icon name="export" :size="14" />
            {{ t('settings.data.exportOpml') }}
          </button>
        </div>
      </div>

      <!-- Full backup & restore -->
      <div class="grouped-inset-box">
        <div class="grouped-inset-row">
          <div class="label-box">
            <span class="label-title">{{ t('settings.data.backup') }}</span>
            <span class="label-desc">{{ t('settings.data.backupDesc') }}</span>
          </div>
          <button class="f-btn" :disabled="backingUp" @click="doExportBackup">
            <Icon name="archivebox" :size="14" />
            {{ t('settings.data.backup') }}
          </button>
        </div>

        <div class="grouped-inset-row">
          <div class="label-box">
            <span class="label-title">{{ t('settings.data.restore') }}</span>
            <span class="label-desc">{{ t('settings.data.restoreDesc') }}</span>
          </div>
          <button class="f-btn" :disabled="backingUp" @click="restoreConfirmVisible = true">
            <Icon name="import" :size="14" />
            {{ t('settings.data.restore') }}
          </button>
        </div>
      </div>

      <div v-if="dataMsg" class="info-banner">
        <Icon name="checkmark" :size="14" color="var(--success)" />
        <span>{{ dataMsg }}</span>
      </div>

      <!-- Restore confirmation -->
      <Modal v-if="restoreConfirmVisible" :title="t('settings.data.restore')" @close="restoreConfirmVisible = false">
        <p class="restore-confirm-text">{{ t('settings.data.confirmRestore') }}</p>
        <template #footer>
          <button class="f-btn" @click="restoreConfirmVisible = false">{{ t('common.cancel') }}</button>
          <button class="f-btn danger" :disabled="backingUp" @click="doImportBackup">
            {{ t('settings.data.restore') }}
          </button>
        </template>
      </Modal>
    </div>
    <div v-else class="tab-body">
      <div class="about-card">
        <div class="about-logo">
          <Icon name="rss" :size="28" color="#ffffff" />
        </div>
        <h3 class="about-title">ZReader</h3>
        <p class="about-ver">{{ t('settings.about.version') }} {{ app.s.version }}</p>
        <p class="about-desc">{{ t('settings.about.desc') }}</p>

        <!-- Updater Box -->
        <div class="updater-box">
          <!-- Idle -->
          <div v-if="updateState.status === 'idle'" class="updater-action">
            <button class="f-btn compact-updater-btn" @click="checkForUpdates()">
              <Icon name="refresh" :size="13" />
              <span>{{ t('settings.about.checkUpdate') }}</span>
            </button>
          </div>

          <!-- Checking -->
          <div v-else-if="updateState.status === 'checking'" class="updater-status checking">
            <Icon name="refresh" :size="14" class="spinning" />
            <span>{{ t('settings.about.checking') }}</span>
          </div>

          <!-- Up to date -->
          <div v-else-if="updateState.status === 'up-to-date'" class="updater-status up-to-date">
            <div class="status-badge success">
              <Icon name="checkmark" :size="13" color="var(--success)" />
              <span>{{ t('settings.about.latest') }}</span>
            </div>
            <button class="f-btn compact-updater-btn secondary" @click="checkForUpdates()">
              <Icon name="refresh" :size="12" />
              <span>{{ t('settings.about.checkUpdate') }}</span>
            </button>
          </div>

          <!-- Available -->
          <div v-else-if="updateState.status === 'available'" class="updater-available-card">
            <div class="update-badge-row">
              <span class="new-ver-badge">{{ t('settings.about.newVersion') }} v{{ updateState.newVersion }}</span>
            </div>
            <p v-if="updateState.releaseNotes" class="release-notes">{{ updateState.releaseNotes }}</p>
            <button class="f-btn primary compact-updater-btn" @click="startDownloadAndInstall()">
              <Icon name="import" :size="13" />
              <span>{{ t('settings.about.updateNow') }}</span>
            </button>
          </div>

          <!-- Downloading -->
          <div v-else-if="updateState.status === 'downloading'" class="updater-downloading">
            <div class="progress-info">
              <span>{{ t('settings.about.downloading') }}…</span>
              <span class="progress-pct">{{ updateState.progress }}%</span>
            </div>
            <div class="progress-bar-track">
              <div class="progress-bar-fill" :style="{ width: `${updateState.progress}%` }"></div>
            </div>
          </div>

          <!-- Downloaded: Ready to Restart -->
          <div v-else-if="updateState.status === 'downloaded'" class="updater-status ready">
            <div class="status-badge success">
              <Icon name="checkmark" :size="13" color="var(--success)" />
              <span>{{ t('settings.about.downloaded') }}</span>
            </div>
            <button class="f-btn primary compact-updater-btn" @click="restartApp()">
              <span>{{ t('settings.about.relaunch') }}</span>
            </button>
          </div>

          <!-- Error -->
          <div v-else-if="updateState.status === 'error'" class="updater-status error">
            <span class="error-text">{{ t('settings.about.updateError') }}: {{ updateState.error }}</span>
            <button class="f-btn compact-updater-btn secondary" @click="checkForUpdates()">
              <span>{{ t('settings.about.retry') }}</span>
            </button>
          </div>
        </div>
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

/* Updater Styles */
.updater-box {
  margin-top: 1rem;
  width: 100%;
  max-width: 22rem;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.5rem;
}

.compact-updater-btn {
  padding: 0.32rem 0.9rem;
  font-size: 0.8rem;
  border-radius: 8px;
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
}

.updater-status {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.5rem;
  font-size: 0.82rem;
  color: var(--text-secondary);
}

.updater-status.checking {
  display: flex;
  flex-direction: row;
  align-items: center;
  gap: 0.45rem;
  color: var(--text-secondary);
}

.spinning {
  animation: spinIcon 1s linear infinite;
}

@keyframes spinIcon {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

.status-badge {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  padding: 0.25rem 0.7rem;
  border-radius: var(--radius-pill);
  font-size: 0.8rem;
}

.status-badge.success {
  background: var(--success-tint, rgba(52, 199, 89, 0.12));
  color: var(--success, #34c759);
  font-weight: 500;
}

.updater-available-card {
  width: 100%;
  padding: 0.85rem 1rem;
  border-radius: 12px;
  background: var(--bg-card);
  border: 0.5px solid var(--border);
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.6rem;
}

.new-ver-badge {
  font-size: 0.82rem;
  font-weight: 600;
  color: var(--accent);
  background: var(--accent-tint);
  padding: 0.2rem 0.65rem;
  border-radius: var(--radius-pill);
}

.release-notes {
  font-size: 0.76rem;
  color: var(--text-secondary);
  max-height: 5rem;
  overflow-y: auto;
  text-align: left;
  width: 100%;
  white-space: pre-wrap;
  margin: 0;
  line-height: 1.4;
}

.updater-downloading {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
}

.progress-info {
  display: flex;
  justify-content: space-between;
  font-size: 0.78rem;
  color: var(--text-secondary);
}

.progress-pct {
  font-weight: 600;
  font-variant-numeric: tabular-nums;
  color: var(--text-primary);
}

.progress-bar-track {
  width: 100%;
  height: 6px;
  background: var(--bg-track);
  border-radius: 3px;
  overflow: hidden;
}

.progress-bar-fill {
  height: 100%;
  background: var(--accent);
  border-radius: 3px;
  transition: width 0.2s ease;
}

.error-text {
  color: var(--danger);
  font-size: 0.78rem;
  text-align: center;
  word-break: break-word;
}

/* General & Rules Tab Styles */
.auth-inputs {
  display: flex;
  gap: 0.4rem;
}

.rules-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  margin-bottom: 0.85rem;
  padding: 0 0.2rem;
}

.rules-hint {
  font-size: 0.8rem;
  color: var(--text-tertiary);
  margin: 0;
}

.rules-toolbar-actions {
  display: flex;
  gap: 0.45rem;
  flex-shrink: 0;
}

.rule-info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 0.18rem;
}

.rule-name-row {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  min-width: 0;
}

.rule-name {
  font-weight: 600;
  font-size: 0.88rem;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.rule-badge {
  font-size: 0.68rem;
  font-weight: 600;
  padding: 0.08rem 0.42rem;
  border-radius: var(--radius-pill);
  background: var(--bg-track);
  color: var(--text-secondary);
  white-space: nowrap;
  flex-shrink: 0;
}

.rule-badge.action {
  background: var(--accent-tint);
  color: var(--accent);
}

.rule-pattern {
  font-family: ui-monospace, 'SF Mono', Consolas, monospace;
  font-size: 0.72rem;
  color: var(--text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.hidden-section {
  margin-top: 1rem;
}

.restore-confirm-text {
  font-size: 0.88rem;
  color: var(--text-secondary);
  line-height: 1.55;
  margin: 0;
}
</style>
