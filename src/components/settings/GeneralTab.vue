<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDataStore } from '../../stores/data'
import { useAppStore } from '../../stores/app'
import * as api from '../../lib/tauri'
import AppleSelect from '../ui/AppleSelect.vue'
import Switch from '../ui/Switch.vue'
import AlertBanner from '../ui/AlertBanner.vue'
import Icon from '../ui/Icon.vue'

const { t } = useI18n()
const data = useDataStore()
const app = useAppStore()

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
  // 若存在抓取失败的源，则优先探测该源以反映真实网络状况。
  const failing = data.sources.find((s) => s.errorCount > 0)
  const target = failing?.url ?? null
  try {
    const ms = await api.testProxy(app.s, target)
    proxyResult.value = t('settings.general.proxyOk', {
      ms,
      target: target ?? t('settings.general.proxyTargetDefault'),
    })
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

onMounted(() => {
  loadStats()
})
</script>

<template>
  <div class="settings-tab-pane">
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
            class="apple-input"
            style="width: 220px"
            placeholder="http://127.0.0.1:7890"
            :value="app.s.proxyUrl"
            @change="app.patch({ proxyUrl: ($event.target as HTMLInputElement).value })"
          />
        </div>
        <div class="grouped-inset-row">
          <div class="label-box">
            <span class="label-title">{{ t('settings.general.proxyAuth') }}</span>
          </div>
          <div class="auth-inputs">
            <input
              class="apple-input"
              :placeholder="t('settings.general.proxyUser')"
              :value="app.s.proxyUsername"
              @change="app.patch({ proxyUsername: ($event.target as HTMLInputElement).value })"
            />
            <input
              class="apple-input"
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

      <div class="grouped-inset-row">
        <div class="label-box">
          <span class="label-title">{{ t('settings.general.faviconThirdParty') }}</span>
          <span class="label-desc">{{ t('settings.general.faviconThirdPartyDesc') }}</span>
        </div>
        <Switch
          :model-value="app.s.faviconThirdParty"
          @update:model-value="app.patch({ faviconThirdParty: $event })"
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

    <AlertBanner v-if="cleanupMsg" type="success" :message="cleanupMsg" />
  </div>
</template>

<style scoped>
.settings-tab-pane {
  display: flex;
  flex-direction: column;
}

.auth-inputs {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 220px;
}

.auth-inputs .apple-input {
  flex: 1;
  width: 0;
  min-width: 0;
  box-sizing: border-box;
}
</style>
