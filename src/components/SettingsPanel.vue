<script setup lang="ts">
import { computed, provide, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import Modal from './ui/Modal.vue'
import Icon from './ui/Icon.vue'
import ConfirmModal from './ui/ConfirmModal.vue'
import SourcesTab from './settings/SourcesTab.vue'
import GeneralTab from './settings/GeneralTab.vue'
import RulesTab from './settings/RulesTab.vue'
import SyncTab from './settings/SyncTab.vue'
import AppTab from './settings/AppTab.vue'
import ShortcutsTab from './settings/ShortcutsTab.vue'
import DataTab from './settings/DataTab.vue'
import AboutTab from './settings/AboutTab.vue'
import { requestConfirmKey, type ConfirmOptions } from './settings/settingsConfirm'

const { t } = useI18n()

const emit = defineEmits<{ close: [] }>()

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

function requestConfirm(options: ConfirmOptions) {
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

provide(requestConfirmKey, requestConfirm)

const tab = ref<'sources' | 'general' | 'rules' | 'sync' | 'app' | 'shortcuts' | 'data' | 'about'>('sources')
const tabs = computed(() => [
  { value: 'sources', label: t('settings.tabs.sources'), icon: 'sources' },
  { value: 'general', label: t('settings.tabs.general'), icon: 'gear' },
  { value: 'rules', label: t('settings.tabs.rules'), icon: 'funnel' },
  { value: 'sync', label: t('settings.tabs.sync'), icon: 'cloud' },
  { value: 'app', label: t('settings.tabs.app'), icon: 'app' },
  { value: 'shortcuts', label: t('settings.tabs.shortcuts'), icon: 'keyboard' },
  { value: 'data', label: t('settings.tabs.data'), icon: 'data' },
  { value: 'about', label: t('settings.tabs.about'), icon: 'info' },
])

const currentTabLabel = computed(() => {
  return tabs.value.find((t) => t.value === tab.value)?.label ?? ''
})
</script>

<template>
  <Modal custom-layout extra-wide @close="emit('close')">
    <div class="settings-layout">
      <!-- 左侧侧边栏 -->
      <aside class="settings-sidebar">
        <div class="settings-sidebar-header">
          <span class="settings-title">{{ t('settings.title') }}</span>
        </div>
        <nav class="settings-nav">
          <button
            v-for="tab_ in tabs"
            :key="tab_.value"
            class="settings-nav-item"
            :class="{ active: tab === tab_.value }"
            @click="tab = tab_.value as typeof tab"
          >
            <Icon :name="tab_.icon" :size="16" class="nav-item-icon" />
            <span class="nav-item-label">{{ tab_.label }}</span>
          </button>
        </nav>
      </aside>

      <!-- 右侧主面板 -->
      <main class="settings-main">
        <header class="settings-main-header">
          <span class="settings-current-title">{{ currentTabLabel }}</span>
          <button class="f-icon-btn close-btn" title="Close" @click="emit('close')">
            <Icon name="xmark" :size="16" />
          </button>
        </header>

        <div class="settings-main-content">
          <SourcesTab v-if="tab === 'sources'" />
          <GeneralTab v-else-if="tab === 'general'" />
          <RulesTab v-else-if="tab === 'rules'" />
          <SyncTab v-else-if="tab === 'sync'" />
          <AppTab v-else-if="tab === 'app'" />
          <ShortcutsTab v-else-if="tab === 'shortcuts'" />
          <DataTab v-else-if="tab === 'data'" />
          <AboutTab v-else />
        </div>

        <footer class="settings-main-footer">
          <button class="f-btn primary" @click="emit('close')">{{ t('common.confirm') }}</button>
        </footer>
      </main>
    </div>
  </Modal>

  <ConfirmModal
    :show="confirmState.show"
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
.settings-layout {
  display: flex;
  width: 100%;
  height: 100%;
  overflow: hidden;
  user-select: none;
}

/* 左侧侧边栏样式 */
.settings-sidebar {
  width: 190px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  background: var(--bg-sidebar);
  border-right: 0.5px solid var(--border);
  overflow: hidden;
}

.settings-sidebar-header {
  height: 52px;
  padding: 0 1.25rem;
  display: flex;
  align-items: center;
  border-bottom: 0.5px solid var(--border-subtle);
  flex-shrink: 0;
}

.settings-title {
  font-size: 1.15rem;
  font-weight: 700;
  letter-spacing: -0.02em;
  color: var(--text-primary);
}

.settings-nav {
  padding: 0.75rem 0.65rem;
  display: flex;
  flex-direction: column;
  gap: 3px;
  overflow-y: auto;
  flex: 1;
}

.settings-nav-item {
  display: flex;
  align-items: center;
  gap: 0.65rem;
  width: 100%;
  height: 36px;
  padding: 0 0.75rem;
  border-radius: 8px;
  border: none;
  background: transparent;
  color: var(--text-primary);
  font-size: 0.86rem;
  font-weight: 500;
  cursor: pointer;
  text-align: left;
  transition: background 0.15s var(--ease), color 0.15s var(--ease);
}

.settings-nav-item:hover {
  background: var(--bg-hover);
}

.settings-nav-item:active {
  background: var(--bg-hover-strong);
}

.settings-nav-item.active {
  background: var(--accent);
  color: #ffffff;
  font-weight: 600;
}

.nav-item-icon {
  flex-shrink: 0;
  color: var(--text-secondary);
  transition: color 0.15s var(--ease);
}

.settings-nav-item.active .nav-item-icon {
  color: #ffffff !important;
}

.nav-item-label {
  flex: 1;
  min-width: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* 右侧主面板样式 */
.settings-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  background: var(--bg);
  overflow: hidden;
}

.settings-main-header {
  height: 52px;
  padding: 0 1.5rem;
  display: flex;
  align-items: center;
  justify-content: space-between;
  border-bottom: 0.5px solid var(--border-subtle);
  flex-shrink: 0;
}

.settings-current-title {
  font-size: 1.12rem;
  font-weight: 700;
  letter-spacing: -0.015em;
  color: var(--text-primary);
}

.close-btn {
  width: 1.7rem;
  height: 1.7rem;
  border-radius: 50%;
  color: var(--text-tertiary);
  display: flex;
  align-items: center;
  justify-content: center;
}

.close-btn:hover {
  background: var(--bg-hover-strong);
  color: var(--text-primary);
}

.settings-main-content {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 1.25rem 1.5rem;
  user-select: text;
}

.settings-tab-pane {
  display: flex;
  flex-direction: column;
}

.settings-main-footer {
  height: 52px;
  padding: 0 1.5rem;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 0.65rem;
  border-top: 0.5px solid var(--border-subtle);
  background: var(--bg-card-secondary);
  flex-shrink: 0;
}

.group-select {
  font-size: 0.8rem;
  padding: 0.28rem 1.8rem 0.28rem 0.65rem;
  max-width: 10rem;
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

.rule-badge {
  font-size: 11px;
  font-weight: 600;
  padding: 1px 6px;
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
</style>
