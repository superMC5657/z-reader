<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useAppStore } from '../../stores/app'
import { createShortcutRecorder, formatKeyDisplay } from './shortcutRecorder'

const { t } = useI18n()
const app = useAppStore()

const recorder = createShortcutRecorder((actionKey, key) => {
  app.setShortcut(actionKey, key)
})
const recordingAction = recorder.recordingAction

function startRecording(actionKey: string) {
  recorder.start(actionKey)
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
</script>

<template>
  <div class="settings-tab-pane">
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
</template>

<style scoped>
.settings-tab-pane {
  display: flex;
  flex-direction: column;
}

/* 快捷键选项卡样式 */
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
