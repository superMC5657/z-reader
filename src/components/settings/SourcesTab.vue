<script setup lang="ts">
import { computed, inject } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDataStore } from '../../stores/data'
import * as api from '../../lib/tauri'
import FeedIcon from '../ui/FeedIcon.vue'
import AppleSelect from '../ui/AppleSelect.vue'
import EmptyState from '../ui/EmptyState.vue'
import Icon from '../ui/Icon.vue'
import { requestConfirmKey } from './settingsConfirm'

const { t } = useI18n()
const data = useDataStore()
const requestConfirm = inject(requestConfirmKey, () => {})

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

function removeSource(id: number) {
  requestConfirm({
    title: t('settings.sources.remove'),
    message: t('settings.sources.confirmRemove'),
    confirmText: t('settings.sources.remove'),
    danger: true,
    onConfirm: () => {
      data.removeSource(id)
    },
  })
}
</script>

<template>
  <div class="settings-tab-pane">
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
    <EmptyState
      v-else
      icon="rss"
      :icon-size="32"
      :title="t('nav.noSources')"
    />
  </div>
</template>

<style scoped>
.settings-tab-pane {
  display: flex;
  flex-direction: column;
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

.remove-btn {
  border-radius: 6px;
}

.remove-btn:hover {
  background: var(--danger-tint);
}
</style>
