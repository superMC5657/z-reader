<script setup lang="ts">
import { computed, reactive, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import type { Group, Rule, RuleActionType, RuleInput, RuleTargetField, Source } from '../types'
import * as api from '../lib/tauri'
import Modal from './ui/Modal.vue'
import AppleSelect from './ui/AppleSelect.vue'
import Switch from './ui/Switch.vue'
import Icon from './ui/Icon.vue'

const props = defineProps<{
  /** null = create a new rule */
  rule: Rule | null
  sources: Source[]
  groups: Group[]
}>()

const emit = defineEmits<{ close: []; saved: [] }>()

const { t } = useI18n()

const form = reactive({
  name: props.rule?.name ?? '',
  pattern: props.rule?.pattern ?? '',
  targetField: (props.rule?.targetField ?? 'title') as RuleTargetField,
  actionType: (props.rule?.actionType ?? 'mark_read') as RuleActionType,
  isCaseSensitive: props.rule?.isCaseSensitive ?? false,
  sourceScope: props.rule?.sourceScope ?? 'all',
})

const saving = ref(false)
const errorMsg = ref('')

const targetOptions = computed(() => [
  { value: 'title', label: t('settings.rules.targetTitle'), icon: 'doc-text-magnifyingglass' },
  { value: 'content', label: t('settings.rules.targetContent'), icon: 'doc-text-magnifyingglass' },
  { value: 'author', label: t('settings.rules.targetAuthor'), icon: 'sources' },
  { value: 'source_url', label: t('settings.rules.targetUrl'), icon: 'open-web' },
  { value: 'any', label: t('settings.rules.targetAny'), icon: 'sparkles' },
])

const actionOptions = computed(() => [
  { value: 'mark_read', label: t('settings.rules.actionMarkRead'), icon: 'checkmark-circle' },
  { value: 'star', label: t('settings.rules.actionStar'), icon: 'star' },
  { value: 'hide', label: t('settings.rules.actionHide'), icon: 'close' },
  { value: 'notify', label: t('settings.rules.actionNotify'), icon: 'bell' },
])

const scopeOptions = computed(() => [
  { value: 'all', label: t('settings.rules.scopeAll'), icon: 'tray-stack' },
  ...props.sources.map((s) => ({
    value: `source:${s.id}`,
    label: `${t('settings.rules.scopeSourcePrefix')}${s.title}`,
    icon: 'rss',
  })),
  ...props.groups.map((g) => ({
    value: `group:${g.id}`,
    label: `${t('settings.rules.scopeGroupPrefix')}${g.name}`,
    icon: 'folder',
  })),
])

const testText = ref('')
const testResult = ref<boolean | null>(null)

function runTest() {
  testResult.value = null
  if (!form.pattern || !testText.value) return
  try {
    const flags = form.isCaseSensitive ? '' : 'i'
    testResult.value = new RegExp(form.pattern, flags).test(testText.value)
  } catch {
    testResult.value = false
  }
}

async function save() {
  errorMsg.value = ''
  if (!form.name.trim()) {
    errorMsg.value = t('settings.rules.errorName')
    return
  }
  if (!form.pattern.trim()) {
    errorMsg.value = t('settings.rules.errorPattern')
    return
  }
  const input: RuleInput = {
    name: form.name.trim(),
    pattern: form.pattern,
    targetField: form.targetField,
    actionType: form.actionType,
    isCaseSensitive: form.isCaseSensitive,
    isEnabled: props.rule?.isEnabled ?? true,
    sourceScope: form.sourceScope,
  }
  saving.value = true
  try {
    if (props.rule) {
      await api.updateRule(props.rule.id, input)
    } else {
      await api.createRule(input)
    }
    emit('saved')
  } catch (err) {
    errorMsg.value = String(err)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <Modal :title="rule ? t('settings.rules.editTitle') : t('settings.rules.newTitle')" @close="emit('close')">
    <div class="rule-form">
      <label class="form-label">{{ t('settings.rules.fieldName') }}</label>
      <input
        v-model="form.name"
        class="apple-text-input"
        :placeholder="t('settings.rules.namePlaceholder')"
      />

      <label class="form-label">{{ t('settings.rules.fieldPattern') }}</label>
      <textarea
        v-model="form.pattern"
        class="rule-pattern-input"
        rows="3"
        spellcheck="false"
        :placeholder="t('settings.rules.patternPlaceholder')"
      ></textarea>

      <div class="form-grid">
        <div>
          <label class="form-label">{{ t('settings.rules.fieldTarget') }}</label>
          <AppleSelect v-model="form.targetField" :options="targetOptions" />
        </div>
        <div>
          <label class="form-label">{{ t('settings.rules.fieldAction') }}</label>
          <AppleSelect v-model="form.actionType" :options="actionOptions" />
        </div>
      </div>

      <label class="form-label">{{ t('settings.rules.fieldScope') }}</label>
      <AppleSelect v-model="form.sourceScope" :options="scopeOptions" />

      <div class="grouped-inset-row standalone-row">
        <div class="label-box">
          <span class="label-title">{{ t('settings.rules.caseSensitive') }}</span>
        </div>
        <Switch v-model="form.isCaseSensitive" />
      </div>

      <div class="rule-test">
        <label class="form-label">{{ t('settings.rules.testTitle') }}</label>
        <input
          v-model="testText"
          class="apple-text-input"
          :placeholder="t('settings.rules.testPlaceholder')"
          @input="runTest"
        />
        <div v-if="testResult !== null" class="test-result" :class="{ match: testResult }">
          <Icon :name="testResult ? 'checkmark-circle' : 'xmark'" :size="14" />
          <span>{{ testResult ? t('settings.rules.testMatch') : t('settings.rules.testNoMatch') }}</span>
        </div>
      </div>

      <div v-if="errorMsg" class="rule-error">{{ errorMsg }}</div>
    </div>

    <template #footer>
      <button class="f-btn" @click="emit('close')">{{ t('common.cancel') }}</button>
      <button class="f-btn primary" :disabled="saving" @click="save">
        {{ saving ? t('settings.rules.saving') : t('common.confirm') }}
      </button>
    </template>
  </Modal>
</template>

<style scoped>
.rule-form {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}

.form-label {
  font-size: 0.76rem;
  font-weight: 600;
  color: var(--text-tertiary);
  margin-top: 0.4rem;
}

.rule-pattern-input {
  font-family: ui-monospace, 'SF Mono', Consolas, monospace;
  font-size: 0.82rem;
  padding: 0.5rem 0.7rem;
  border-radius: 8px;
  border: 1px solid var(--border-subtle);
  background: var(--bg-input);
  color: var(--text-primary);
  resize: vertical;
  min-height: 3.2rem;
  outline: none;
}

.rule-pattern-input:focus {
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-tint);
}

.form-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 0.8rem;
}

.standalone-row {
  border-radius: 10px;
  background: var(--bg-card);
  border: 0.5px solid var(--border);
  margin-top: 0.7rem;
}

.rule-test {
  margin-top: 0.7rem;
}

.test-result {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  font-size: 0.8rem;
  color: var(--danger);
  margin-top: 0.35rem;
}

.test-result.match {
  color: var(--success);
}

.rule-error {
  margin-top: 0.6rem;
  padding: 0.55rem 0.8rem;
  border-radius: 8px;
  background: var(--danger-tint);
  color: var(--danger);
  font-size: 0.8rem;
}
</style>
