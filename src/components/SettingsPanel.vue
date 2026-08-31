<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDataStore } from '../stores/data'
import { useAppStore } from '../stores/app'
import * as api from '../lib/tauri'
import Modal from './ui/Modal.vue'
import { LOCALES } from '../i18n'

const { t } = useI18n()
const data = useDataStore()
const app = useAppStore()

const emit = defineEmits<{ close: [] }>()

const tab = ref<'sources' | 'app' | 'data' | 'about'>('sources')
const tabs = computed(() => [
  { value: 'sources', label: t('settings.tabs.sources') },
  { value: 'app', label: t('settings.tabs.app') },
  { value: 'data', label: t('settings.tabs.data') },
  { value: 'about', label: t('settings.tabs.about') },
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
  target.value = ''}

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
</script>

<template>
  <Modal :title="t('settings.title')" wide @close="emit('close')">
    <div class="settings-tabs">
      <button
        v-for="tab_ in tabs"
        :key="tab_.value"
        class="stab"
        :class="{ active: tab === tab_.value }"
        @click="tab = tab_.value as typeof tab"
      >
        {{ tab_.label }}
      </button>
    </div>

    <div v-if="tab === 'sources'" class="tab-body">
      <div v-for="s in data.sources" :key="s.id" class="source-row">
        <div class="source-info">
          <span class="source-title">{{ s.title }}</span>
          <span class="source-url">{{ s.url }}</span>
        </div>
        <select :value="s.groupId ?? ''" @change="setSourceGroup(s.id, $event)">
          <option value="">{{ t('settings.sources.ungrouped') }}</option>
          <option v-for="g in data.groups" :key="g.id" :value="String(g.id)">{{ g.name }}</option>
        </select>
        <button class="f-btn danger" @click="removeSource(s.id)">{{ t('settings.sources.remove') }}</button>
      </div>
      <p v-if="!data.sources.length" class="hint">{{ t('nav.noSources') }}</p>
    </div>

    <div v-else-if="tab === 'app'" class="tab-body">
      <div class="form-row">
        <label>{{ t('settings.app.theme') }}</label>
        <select :value="app.s.theme" @change="app.patch({ theme: ($event.target as HTMLSelectElement).value as any })">
          <option value="system">{{ t('settings.app.themeSystem') }}</option>
          <option value="light">{{ t('settings.app.themeLight') }}</option>
          <option value="dark">{{ t('settings.app.themeDark') }}</option>
        </select>
      </div>
      <div class="form-row">
        <label>{{ t('settings.app.view') }}</label>
        <select :value="app.s.view" @change="app.patch({ view: ($event.target as HTMLSelectElement).value as any })">
          <option value="cards">{{ t('toolbar.views.cards') }}</option>
          <option value="list">{{ t('toolbar.views.list') }}</option>
          <option value="magazine">{{ t('toolbar.views.magazine') }}</option>
          <option value="compact">{{ t('toolbar.views.compact') }}</option>
        </select>
      </div>
      <div class="form-row">
        <label>{{ t('settings.app.language') }}</label>
        <select :value="app.s.locale" @change="app.patch({ locale: ($event.target as HTMLSelectElement).value })">
          <option v-for="l in LOCALES" :key="l.value" :value="l.value">{{ l.label }}</option>
        </select>
      </div>
      <div class="form-row">
        <label>{{ t('settings.app.fontSize') }}: {{ app.s.fontSize }}px</label>
        <input
          type="range"
          min="12"
          max="22"
          step="1"
          :value="app.s.fontSize"
          @change="app.patch({ fontSize: Number(($event.target as HTMLInputElement).value) })"
        />
      </div>
      <div class="form-row">
        <label>{{ t('settings.app.fetchInterval') }}</label>
        <input
          type="number"
          min="1"
          max="1440"
          :value="app.s.fetchInterval"
          @change="app.patch({ fetchInterval: Math.max(1, Number(($event.target as HTMLInputElement).value) || 30) })"
        />
      </div>
      <div class="form-row check">
        <label>
          <input
            type="checkbox"
            :checked="app.showCover"
            @change="app.patch({ viewConfigs: ($event.target as HTMLInputElement).checked ? app.s.viewConfigs | 1 : app.s.viewConfigs & ~1 })"
          />
          {{ t('settings.app.showCover') }}
        </label>
        <label>
          <input
            type="checkbox"
            :checked="app.showSnippet"
            @change="app.patch({ viewConfigs: ($event.target as HTMLInputElement).checked ? app.s.viewConfigs | 2 : app.s.viewConfigs & ~2 })"
          />
          {{ t('settings.app.showSnippet') }}
        </label>
        <label>
          <input
            type="checkbox"
            :checked="app.fadeRead"
            @change="app.patch({ viewConfigs: ($event.target as HTMLInputElement).checked ? app.s.viewConfigs | 4 : app.s.viewConfigs & ~4 })"
          />
          {{ t('settings.app.fadeRead') }}
        </label>
      </div>
    </div>

    <div v-else-if="tab === 'data'" class="tab-body">
      <div class="data-actions">
        <button class="f-btn" @click="opmlInput?.click()">{{ t('settings.data.importOpml') }}</button>
        <button class="f-btn" @click="exportOpml">{{ t('settings.data.exportOpml') }}</button>
        <input ref="opmlInput" type="file" accept=".opml,.xml" hidden @change="importOpml" />
      </div>
      <p v-if="dataMsg" class="hint">{{ dataMsg }}</p>
    </div>

    <div v-else class="tab-body">
      <p><strong>ZReader</strong></p>
      <p>{{ t('settings.about.version') }}: {{ app.s.version }}</p>
      <p class="hint">{{ t('settings.about.desc') }}</p>
    </div>

    <template #footer>
      <button class="f-btn primary" @click="emit('close')">{{ t('common.confirm') }}</button>
    </template>
  </Modal>
</template>

<style scoped>
.settings-tabs {
  display: inline-flex;
  align-self: center;
  background: var(--bg-track);
  border-radius: 9px;
  margin: 0.4rem auto 0.9rem;
  gap: 1px;
  padding: 2px;
}

.stab {
  padding: 0.26rem 1rem;
  font-size: 0.82rem;
  font-weight: 500;
  color: var(--text-secondary);
  transition: all 0.15s var(--ease);
}

.stab.active {
  background: var(--bg-card);
  color: var(--text-primary);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.12), 0 0 0 0.5px var(--border);
  border-radius: 7px;
}

[data-theme='dark'] .stab.active {
  background: rgba(110, 110, 115, 0.55);
}

.tab-body {
  padding-top: 0.3rem;
  min-height: 16rem;
}

.source-row {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  padding: 0.45rem 0;
  border-bottom: 0.5px solid var(--border);
}

.source-info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}

.source-title {
  font-weight: 600;
  font-size: 0.88rem;
}

.source-url {
  font-size: 0.72rem;
  color: var(--text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.data-actions {
  display: flex;
  gap: 0.55rem;
}

.hint {
  color: var(--text-tertiary);
  font-size: 0.82rem;
  margin-top: 0.6rem;
}

.form-row.check label {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  font-size: 0.9rem;
  color: var(--text-primary);
  margin-bottom: 0.45rem;
}
</style>
