<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { useAppStore } from '../../stores/app'
import {
  updateState,
  checkForUpdates,
  startDownloadAndInstall,
  restartApp,
} from '../../lib/updater'
import AlertBanner from '../ui/AlertBanner.vue'
import Badge from '../ui/Badge.vue'
import Icon from '../ui/Icon.vue'

const { t } = useI18n()
const app = useAppStore()
</script>

<template>
  <div class="settings-tab-pane">
    <div class="about-card">
      <div class="about-logo">
        <Icon name="rss" :size="28" color="#ffffff" />
      </div>
      <h3 class="about-title">ZReader</h3>
      <p class="about-ver">{{ t('settings.about.version') }} {{ app.s.version }}</p>
      <p class="about-desc">{{ t('settings.about.desc') }}</p>

      <!-- 检查更新模块 -->
      <div class="updater-box">
        <!-- 空闲状态 -->
        <div v-if="updateState.status === 'idle'" class="updater-action">
          <button class="f-btn compact-updater-btn" @click="checkForUpdates()">
            <Icon name="refresh" :size="13" />
            <span>{{ t('settings.about.checkUpdate') }}</span>
          </button>
        </div>

        <!-- 检查中状态 -->
        <div v-else-if="updateState.status === 'checking'" class="updater-status checking">
          <Icon name="refresh" :size="14" class="spinning" />
          <span>{{ t('settings.about.checking') }}</span>
        </div>

        <!-- 已是最新状态 -->
        <div v-else-if="updateState.status === 'up-to-date'" class="updater-status up-to-date">
          <Badge variant="success">
            <Icon name="checkmark" :size="13" color="var(--success)" style="margin-right: 4px;" />
            <span>{{ t('settings.about.latest') }}</span>
          </Badge>
          <button class="f-btn compact-updater-btn secondary" @click="checkForUpdates()">
            <Icon name="refresh" :size="12" />
            <span>{{ t('settings.about.checkUpdate') }}</span>
          </button>
        </div>

        <!-- 有新版本可用状态 -->
        <div v-else-if="updateState.status === 'available'" class="updater-available-card">
          <div class="update-badge-row">
            <Badge variant="tint">{{ t('settings.about.newVersion') }} v{{ updateState.newVersion }}</Badge>
          </div>
          <p v-if="updateState.releaseNotes" class="release-notes">{{ updateState.releaseNotes }}</p>
          <button class="f-btn primary compact-updater-btn" @click="startDownloadAndInstall()">
            <Icon name="import" :size="13" />
            <span>{{ t('settings.about.updateNow') }}</span>
          </button>
        </div>

        <!-- 下载中状态 -->
        <div v-else-if="updateState.status === 'downloading'" class="updater-downloading">
          <div class="progress-info">
            <span>{{ t('settings.about.downloading') }}…</span>
            <span class="progress-pct">{{ updateState.progress }}%</span>
          </div>
          <div class="progress-bar-track">
            <div class="progress-bar-fill" :style="{ width: `${updateState.progress}%` }"></div>
          </div>
        </div>

        <!-- 下载完成：准备重启状态 -->
        <div v-else-if="updateState.status === 'downloaded'" class="updater-status ready">
          <Badge variant="success">
            <Icon name="checkmark" :size="13" color="var(--success)" style="margin-right: 4px;" />
            <span>{{ t('settings.about.downloaded') }}</span>
          </Badge>
          <button class="f-btn primary compact-updater-btn" @click="restartApp()">
            <span>{{ t('settings.about.relaunch') }}</span>
          </button>
        </div>

        <!-- 更新出错状态 -->
        <AlertBanner
          v-else-if="updateState.status === 'error'"
          type="error"
          :message="`${t('settings.about.updateError')}: ${updateState.error}`"
          retryable
          :retry-text="t('settings.about.retry')"
          @retry="checkForUpdates()"
        />
      </div>
    </div>
  </div>
</template>

<style scoped>
.settings-tab-pane {
  display: flex;
  flex-direction: column;
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

/* 检查更新模块样式 */
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
</style>
