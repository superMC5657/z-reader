<script setup lang="ts">
import { inject, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDataStore } from '../../stores/data'
import * as api from '../../lib/tauri'
import type { Item, Rule, RuleBackfillResult } from '../../types'
import Switch from '../ui/Switch.vue'
import EmptyState from '../ui/EmptyState.vue'
import AlertBanner from '../ui/AlertBanner.vue'
import Icon from '../ui/Icon.vue'
import Badge from '../ui/Badge.vue'
import RuleEditDialog from '../RuleEditDialog.vue'
import { requestConfirmKey, requestDestructiveConfirm } from './settingsConfirm'

const { t } = useI18n()
const data = useDataStore()
const requestConfirm = inject(requestConfirmKey, () => {})

const rules = ref<Rule[]>([])
const rulesLoading = ref(false)
const showRuleEditor = ref(false)
const editingRule = ref<Rule | null>(null)
const backfillMsg = ref('')
const hiddenItems = ref<Item[]>([])

async function loadRulesTab() {
  rulesLoading.value = true
  try {
    const [rs, hidden] = await Promise.all([
      api.getRules(),
      api.getItems({ filter: 3, limit: 100 }),
    ])
    rules.value = rs
    hiddenItems.value = hidden
  } finally {
    rulesLoading.value = false
  }
}

function openRuleEditor(rule: Rule | null) {
  editingRule.value = rule
  showRuleEditor.value = true
}

function onRuleSaved() {
  showRuleEditor.value = false
  loadRulesTab()
}

async function toggleRule(rule: Rule, enabled: boolean) {
  await api.updateRule(rule.id, {
    name: rule.name,
    pattern: rule.pattern,
    targetField: rule.targetField,
    actionType: rule.actionType,
    isCaseSensitive: rule.isCaseSensitive,
    isEnabled: enabled,
    sourceScope: rule.sourceScope,
  })
  rule.isEnabled = enabled
}

function removeRule(id: number) {
  requestDestructiveConfirm(requestConfirm, {
    title: t('settings.rules.delete'),
    message: t('settings.rules.confirmDelete'),
    confirmText: t('settings.rules.delete'),
    onConfirm: async () => {
      await api.deleteRule(id)
      await loadRulesTab()
    },
  })
}

async function unhideItem(id: number) {
  await api.setItemHidden(id, false)
  hiddenItems.value = hiddenItems.value.filter((i) => i.id !== id)
}

async function applyBackfill() {
  backfillMsg.value = ''
  try {
    const r: RuleBackfillResult = await api.applyRulesBackfill()
    backfillMsg.value = t('settings.rules.backfillDone', {
      read: r.markedRead,
      starred: r.starred,
      hidden: r.hidden,
    })
    await loadRulesTab()
  } catch (err) {
    backfillMsg.value = String(err)
  }
}

function scopeLabel(scope: string): string {
  if (scope === 'all') return t('settings.rules.scopeAll')
  const [, idStr] = scope.split(':')
  const id = Number(idStr)
  if (scope.startsWith('source:')) {
    return `${t('settings.rules.scopeSourcePrefix')}${data.sourceById(id)?.title ?? id}`
  }
  const group = data.groups.find((g) => g.id === id)
  return `${t('settings.rules.scopeGroupPrefix')}${group?.name ?? id}`
}

onMounted(() => {
  loadRulesTab()
})
</script>

<template>
  <div class="settings-tab-pane">
    <div class="rules-toolbar">
      <p class="rules-hint">{{ t('settings.rules.hint') }}</p>
      <div class="rules-toolbar-actions">
        <button class="f-btn compact-btn" @click="applyBackfill">
          <Icon name="sparkles" :size="13" />
          {{ t('settings.rules.applyBackfill') }}
        </button>
        <button class="f-btn primary compact-btn" @click="openRuleEditor(null)">
          <Icon name="plus" :size="13" />
          {{ t('settings.rules.new') }}
        </button>
      </div>
    </div>

    <div v-if="rules.length" class="grouped-inset-box">
      <div v-for="r in rules" :key="r.id" class="grouped-inset-row">
        <div class="rule-info">
          <div class="rule-name-row">
            <span class="rule-name">{{ r.name }}</span>
            <Badge variant="tint">{{ t(`settings.rules.action_${r.actionType}`) }}</Badge>
            <Badge>{{ scopeLabel(r.sourceScope) }}</Badge>
          </div>
          <code class="rule-pattern">{{ r.pattern }}</code>
        </div>
        <div class="source-actions">
          <Switch :model-value="r.isEnabled" @update:model-value="toggleRule(r, $event)" />
          <button class="f-icon-btn" :title="t('feed.rename')" @click="openRuleEditor(r)">
            <Icon name="pencil" :size="14" />
          </button>
          <button class="f-icon-btn remove-btn" :title="t('settings.rules.delete')" @click="removeRule(r.id)">
            <Icon name="trash" :size="15" color="var(--danger)" />
          </button>
        </div>
      </div>
    </div>
    <EmptyState
      v-else
      icon="funnel"
      :icon-size="32"
      :title="t('settings.rules.empty')"
    />

    <AlertBanner v-if="backfillMsg" type="success" :message="backfillMsg" />

    <div v-if="hiddenItems.length" class="hidden-section">
      <div class="shortcut-group-title">{{ t('settings.rules.hiddenSection', { n: hiddenItems.length }) }}</div>
      <div class="grouped-inset-box">
        <div v-for="h in hiddenItems" :key="h.id" class="grouped-inset-row">
          <div class="rule-info">
            <span class="rule-name">{{ h.title }}</span>
          </div>
          <button class="f-btn compact-btn" @click="unhideItem(h.id)">
            {{ t('settings.rules.unhide') }}
          </button>
        </div>
      </div>
    </div>

    <RuleEditDialog
      v-if="showRuleEditor"
      :rule="editingRule"
      :sources="data.sources"
      :groups="data.groups"
      @close="showRuleEditor = false"
      @saved="onRuleSaved"
    />
  </div>
</template>

<style scoped>
.settings-tab-pane {
  display: flex;
  flex-direction: column;
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

.compact-btn {
  font-size: 0.76rem;
  padding: 0.24rem 0.65rem;
  border-radius: 6px;
  flex-shrink: 0;
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

.rules-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 12px;
  padding: 0 3px;
}

.rules-hint {
  font-size: 12px;
  color: var(--text-tertiary);
  margin: 0;
}

.rules-toolbar-actions {
  display: flex;
  gap: 7px;
  flex-shrink: 0;
}

.rule-info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.rule-name-row {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}

.rule-name {
  font-weight: 600;
  font-size: 13px;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.rule-pattern {
  font-family: ui-monospace, 'SF Mono', Consolas, monospace;
  font-size: 11px;
  color: var(--text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.hidden-section {
  margin-top: 16px;
}
</style>
