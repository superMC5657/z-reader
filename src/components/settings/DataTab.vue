<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDataStore } from '../../stores/data'
import * as api from '../../lib/tauri'
import { restartApp } from '../../lib/updater'
import AlertBanner from '../ui/AlertBanner.vue'
import Icon from '../ui/Icon.vue'
import ConfirmModal from '../ui/ConfirmModal.vue'

const { t } = useI18n()
const data = useDataStore()

const dataMsg = ref('')
const opmlInput = ref<HTMLInputElement | null>(null)

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

// ---------- 完整备份与恢复 ----------

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
      // 恢复成功后会替换数据库；重启应用以完成全新重载。
      await restartApp()
    }
  } catch (err) {
    dataMsg.value = String(err)
  } finally {
    backingUp.value = false
  }
}
</script>

<template>
  <div class="settings-tab-pane">
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

    <!-- 完整备份与恢复 -->
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

    <AlertBanner v-if="dataMsg" type="success" :message="dataMsg" />

    <!-- 恢复确认弹窗 -->
    <ConfirmModal
      v-if="restoreConfirmVisible"
      :title="t('settings.data.restore')"
      :message="t('settings.data.confirmRestore')"
      :confirm-text="t('settings.data.restore')"
      danger
      :loading="backingUp"
      @confirm="doImportBackup"
      @cancel="restoreConfirmVisible = false"
      @close="restoreConfirmVisible = false"
    />
  </div>
</template>

<style scoped>
.settings-tab-pane {
  display: flex;
  flex-direction: column;
}

.restore-confirm-text {
  font-size: 13px;
  color: var(--text-secondary);
  line-height: 1.55;
  margin: 0;
}
</style>
