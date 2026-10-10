<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDataStore } from '../../stores/data'
import { useAppStore } from '../../stores/app'
import * as api from '../../lib/tauri'
import type { SyncStatus } from '../../types'
import { formatFullTime } from '../../lib/time'
import AlertBanner from '../ui/AlertBanner.vue'
import Badge from '../ui/Badge.vue'
import Icon from '../ui/Icon.vue'
import ConfirmModal from '../ui/ConfirmModal.vue'

const { t } = useI18n()
const data = useDataStore()
const app = useAppStore()

const syncServerUrl = ref('')
const syncUsername = ref('')
const syncPassword = ref('')
const syncConnecting = ref(false)
const syncErrorMsg = ref('')
const syncStatusInfo = ref<SyncStatus | null>(null)
const syncingNow = ref(false)
const syncResultMsg = ref('')
const syncDisconnectVisible = ref(false)

async function loadSyncStatus() {
  if (!app.s.syncAccount) return
  try {
    syncStatusInfo.value = await api.syncStatus()
  } catch {
    syncStatusInfo.value = null
  }
}

async function doSyncLogin() {
  syncErrorMsg.value = ''
  if (!syncServerUrl.value.trim() || !syncUsername.value.trim() || !syncPassword.value) {
    syncErrorMsg.value = t('settings.sync.errorRequired')
    return
  }
  syncConnecting.value = true
  try {
    const count = await api.syncLogin(syncServerUrl.value, syncUsername.value, syncPassword.value)
    await app.refresh()
    await Promise.all([data.loadSources(), data.loadGroups(), data.loadItems()])
    syncResultMsg.value = t('settings.sync.connected', { n: count })
    await loadSyncStatus()
  } catch (err) {
    syncErrorMsg.value = String(err)
  } finally {
    syncConnecting.value = false
  }
}

async function doSyncNow() {
  syncingNow.value = true
  syncResultMsg.value = ''
  try {
    const r = await api.syncNow()
    syncResultMsg.value = t('settings.sync.syncDone', {
      pull: r.newItems,
      push: r.pushed,
      fail: r.failures,
    })
    await Promise.all([data.loadSources(), data.loadGroups(), data.loadItems()])
    await loadSyncStatus()
  } catch (err) {
    syncResultMsg.value = String(err)
  } finally {
    syncingNow.value = false
  }
}

async function doSyncLogout() {
  syncDisconnectVisible.value = false
  try {
    await api.syncLogout()
    await app.refresh()
    syncStatusInfo.value = null
    syncResultMsg.value = ''
  } catch (err) {
    syncResultMsg.value = String(err)
  }
}

onMounted(() => {
  loadSyncStatus()
})
</script>

<template>
  <div class="settings-tab-pane">
    <template v-if="!app.s.syncAccount">
      <div class="sync-form">
        <div class="form-field">
          <label class="form-label">{{ t('settings.sync.serverUrl') }}</label>
          <input
            v-model="syncServerUrl"
            class="apple-input"
            placeholder="https://example.com/api/greader.php"
            spellcheck="false"
          />
          <p class="sync-hint">{{ t('settings.sync.serverHint') }}</p>
        </div>

        <div class="form-grid">
          <div class="form-field">
            <label class="form-label">{{ t('settings.sync.username') }}</label>
            <input v-model="syncUsername" class="apple-input" autocomplete="off" spellcheck="false" />
          </div>
          <div class="form-field">
            <label class="form-label">{{ t('settings.sync.password') }}</label>
            <input v-model="syncPassword" class="apple-input" type="password" autocomplete="off" />
          </div>
        </div>

        <AlertBanner v-if="syncErrorMsg" type="error" :message="syncErrorMsg" />

        <button class="f-btn primary sync-connect-btn" :disabled="syncConnecting" @click="doSyncLogin">
          <Icon name="cloud" :size="14" />
          {{ syncConnecting ? t('settings.sync.connecting') : t('settings.sync.connect') }}
        </button>
        <p class="sync-hint">{{ t('settings.sync.modeHint') }}</p>
        <p class="sync-hint warning">{{ t('settings.sync.plaintextWarning') }}</p>
      </div>
    </template>

    <template v-else>
      <div class="grouped-inset-box">
        <div class="grouped-inset-row">
          <div class="label-box">
            <span class="label-title">{{ t('settings.sync.account') }}</span>
            <span class="label-desc">{{ app.s.syncAccount.serverUrl }}</span>
          </div>
          <Badge variant="tint">Google Reader API</Badge>
        </div>
        <div class="grouped-inset-row">
          <div class="label-box">
            <span class="label-title">{{ t('settings.sync.lastSync') }}</span>
          </div>
          <span class="sync-time">
            {{ syncStatusInfo?.lastSync ? formatFullTime(syncStatusInfo.lastSync) : t('settings.sync.never') }}
          </span>
        </div>
        <div class="grouped-inset-row">
          <div class="label-box">
            <span class="label-title">{{ t('settings.sync.pendingQueue') }}</span>
            <span class="label-desc">{{ t('settings.sync.pendingQueueDesc') }}</span>
          </div>
          <Badge>{{ syncStatusInfo?.queueLen ?? 0 }}</Badge>
        </div>
      </div>

      <div class="grouped-inset-box">
        <div class="grouped-inset-row">
          <div class="label-box">
            <span class="label-title">{{ t('settings.sync.syncNow') }}</span>
            <span class="label-desc">{{ t('settings.sync.syncNowDesc') }}</span>
          </div>
          <button class="f-btn" :disabled="syncingNow" @click="doSyncNow">
            <Icon name="refresh" :size="14" />
            {{ syncingNow ? t('common.loading') : t('settings.sync.syncNow') }}
          </button>
        </div>
        <div class="grouped-inset-row">
          <div class="label-box">
            <span class="label-title">{{ t('settings.sync.disconnect') }}</span>
            <span class="label-desc">{{ t('settings.sync.disconnectDesc') }}</span>
          </div>
          <button class="f-btn" @click="syncDisconnectVisible = true">
            {{ t('settings.sync.disconnect') }}
          </button>
        </div>
      </div>

      <AlertBanner v-if="syncResultMsg" type="success" :message="syncResultMsg" />

      <ConfirmModal
        v-if="syncDisconnectVisible"
        :title="t('settings.sync.disconnect')"
        :message="t('settings.sync.confirmDisconnect')"
        :confirm-text="t('settings.sync.disconnect')"
        danger
        @confirm="doSyncLogout"
        @cancel="syncDisconnectVisible = false"
        @close="syncDisconnectVisible = false"
      />
    </template>
  </div>
</template>

<style scoped>
.settings-tab-pane {
  display: flex;
  flex-direction: column;
}

/* 云同步选项卡样式 */
.sync-form {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 0 3px;
  width: 100%;
}

.sync-form .apple-input {
  width: 100%;
  box-sizing: border-box;
}

.form-field {
  display: flex;
  flex-direction: column;
  gap: 5px;
  width: 100%;
}

.form-label {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-secondary);
}

.form-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
  width: 100%;
}

.sync-hint {
  font-size: 11px;
  color: var(--text-tertiary);
  margin: 2px 0 5px;
  line-height: 1.45;
}

.sync-hint.warning {
  color: var(--text-tertiary);
  border-left: 2px solid var(--border-strong);
  padding-left: 8px;
}

.sync-connect-btn {
  align-self: flex-start;
  margin-top: 10px;
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.sync-provider-badge {
  font-size: 11px;
  font-weight: 600;
  color: var(--accent);
  background: var(--accent-tint);
  padding: 2px 8px;
  border-radius: var(--radius-pill);
  white-space: nowrap;
  flex-shrink: 0;
}

.sync-time {
  font-size: 12px;
  color: var(--text-secondary);
  font-variant-numeric: tabular-nums;
}
</style>
