<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDataStore } from '../stores/data'
import { useAppStore } from '../stores/app'
import * as api from '../lib/tauri'
import Modal from './ui/Modal.vue'
import Icon from './ui/Icon.vue'
import Switch from './ui/Switch.vue'
import { LOCALES } from '../i18n'

const { t } = useI18n()
const data = useDataStore()
const app = useAppStore()

const emit = defineEmits<{ close: [] }>()

const tab = ref<'sources' | 'app' | 'data' | 'about'>('sources')
const tabs = computed(() => [
  { value: 'sources', label: t('settings.tabs.sources'), icon: 'sources' },
  { value: 'app', label: t('settings.tabs.app'), icon: 'app' },
  { value: 'data', label: t('settings.tabs.data'), icon: 'data' },
  { value: 'about', label: t('settings.tabs.about'), icon: 'info' },
])

const dataMsg = ref('')
const opmlInput = ref<HTMLInputElement | null>(null)

function setSourceGroup(id: number, e: Event) {
  const value = (e.target as HTMLSelectElement).value
  api
    .setSourceGroup(id, value === '' ? null : Number(value))
    .then(() => Promise.all([data.loadSources(), data.loadGroups()]))
}

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
            <span class="source-title">{{ s.title }}</span>
            <span class="source-url">{{ s.url }}</span>
          </div>
          <div class="source-actions">
            <select :value="s.groupId ?? ''" class="group-select" @change="setSourceGroup(s.id, $event)">
              <option value="">{{ t('settings.sources.ungrouped') }}</option>
              <option v-for="g in data.groups" :key="g.id" :value="String(g.id)">{{ g.name }}</option>
            </select>
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
          <select :value="app.s.theme" @change="app.patch({ theme: ($event.target as HTMLSelectElement).value as any })">
            <option value="system">{{ t('settings.app.themeSystem') }}</option>
            <option value="light">{{ t('settings.app.themeLight') }}</option>
            <option value="dark">{{ t('settings.app.themeDark') }}</option>
          </select>
        </div>

        <!-- Default View -->
        <div class="grouped-inset-row">
          <div class="label-box">
            <span class="label-title">{{ t('settings.app.view') }}</span>
          </div>
          <select :value="app.s.view" @change="app.patch({ view: ($event.target as HTMLSelectElement).value as any })">
            <option value="cards">{{ t('toolbar.views.cards') }}</option>
            <option value="list">{{ t('toolbar.views.list') }}</option>
            <option value="magazine">{{ t('toolbar.views.magazine') }}</option>
            <option value="compact">{{ t('toolbar.views.compact') }}</option>
          </select>
        </div>

        <!-- Language -->
        <div class="grouped-inset-row">
          <div class="label-box">
            <span class="label-title">{{ t('settings.app.language') }}</span>
          </div>
          <select :value="app.s.locale" @change="app.patch({ locale: ($event.target as HTMLSelectElement).value })">
            <option v-for="l in LOCALES" :key="l.value" :value="l.value">{{ l.label }}</option>
          </select>
        </div>

        <!-- Font Size Slider -->
        <div class="grouped-inset-row">
          <div class="label-box">
            <span class="label-title">{{ t('settings.app.fontSize') }}</span>
          </div>
          <div class="slider-row">
            <input
              type="range"
              min="12"
              max="22"
              step="1"
              :value="app.s.fontSize"
              @input="app.patch({ fontSize: Number(($event.target as HTMLInputElement).value) })"
            />
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
  gap: 0.15rem;
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
  gap: 0.75rem;
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
</style>
