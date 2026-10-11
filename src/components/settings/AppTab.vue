<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useAppStore } from '../../stores/app'
import AppleSelect from '../ui/AppleSelect.vue'
import Switch from '../ui/Switch.vue'
import StepperSlider from '../ui/StepperSlider.vue'
import { LOCALES } from '../../i18n'

const { t } = useI18n()
const app = useAppStore()

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

function toggleViewConfig(bit: number, val: boolean) {
  if (val) {
    app.patch({ viewConfigs: app.s.viewConfigs | bit })
  } else {
    app.patch({ viewConfigs: app.s.viewConfigs & ~bit })
  }
}

// 取值范围（钳位/回落）由 settings store 的 validate 半区唯一拥有，
// tab 只做薄适配传原值，不在模板与脚本里各备一份 clamp。
function adjustUiScale(delta: number) {
  const cur = app.s.uiScale || 100
  app.patch({ uiScale: cur + delta })
}

function adjustFontSize(delta: number) {
  const cur = app.s.fontSize || 16
  app.patch({ fontSize: cur + delta })
}
</script>

<template>
  <div class="settings-tab-pane">
    <div class="grouped-inset-box">
      <!-- 主题外观 -->
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

      <!-- 默认视图 -->
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

      <!-- 阅读模式 -->
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

      <!-- 界面语言 -->
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

      <!-- 界面缩放 / 分辨率 -->
      <div class="grouped-inset-row">
        <div class="label-box">
          <span class="label-title">{{ t('settings.app.uiScale') }}</span>
          <span class="label-desc">{{ t('settings.app.uiScaleDesc') }}</span>
        </div>
        <StepperSlider
          :model-value="app.s.uiScale || 100"
          :min="80"
          :max="150"
          :step="5"
          unit="%"
          :reset-value="100"
          :badge-title="t('settings.shortcuts.reset')"
          @update:model-value="app.patch({ uiScale: $event })"
        />
      </div>

      <!-- 字体大小滑块 -->
      <div class="grouped-inset-row">
        <div class="label-box">
          <span class="label-title">{{ t('settings.app.fontSize') }}</span>
        </div>
        <StepperSlider
          :model-value="app.s.fontSize"
          :min="12"
          :max="22"
          :step="1"
          unit="px"
          decrease-label="A-"
          increase-label="A+"
          @update:model-value="app.patch({ fontSize: $event })"
        />
      </div>

      <!-- 后台刷新间隔 -->
      <div class="grouped-inset-row">
        <div class="label-box">
          <span class="label-title">{{ t('settings.app.fetchInterval') }}</span>
        </div>
        <input
          type="number"
          min="1"
          max="1440"
          style="width: 100px; text-align: center"
          :value="app.s.fetchInterval"
          @change="app.patch({ fetchInterval: Number(($event.target as HTMLInputElement).value) })"
        />
      </div>
    </div>

    <!-- 视图配置开关 -->
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
</template>

<style scoped>
.settings-tab-pane {
  display: flex;
  flex-direction: column;
}
</style>
