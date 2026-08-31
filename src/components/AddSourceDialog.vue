<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDataStore } from '../stores/data'
import Modal from './ui/Modal.vue'
import Icon from './ui/Icon.vue'
import AppleSelect from './ui/AppleSelect.vue'

const { t } = useI18n()
const data = useDataStore()

const emit = defineEmits<{ close: []; added: [] }>()

const url = ref('')
const groupId = ref<string>('')
const adding = ref(false)
const error = ref('')

const hasGroups = computed(() => data.groups.length > 0)
const groupOptions = computed(() => [
  { value: '', label: t('addSource.none'), icon: 'rss' },
  ...data.groups.map((g) => ({ value: String(g.id), label: g.name, icon: 'folder' })),
])

async function submit() {
  if (!url.value.trim() || adding.value) return
  adding.value = true
  error.value = ''
  try {
    await data.addSource(url.value.trim(), groupId.value ? Number(groupId.value) : null)
    emit('close')
    emit('added')
  } catch (e) {
    error.value = String(e)
  } finally {
    adding.value = false
  }
}
</script>

<template>
  <Modal :title="t('addSource.title')" @close="emit('close')">
    <div class="form-container">
      <div class="form-row">
        <label class="form-label">{{ t('addSource.urlLabel') }}</label>
        <input
          v-model="url"
          class="apple-input"
          :placeholder="t('addSource.urlPlaceholder')"
          autofocus
          @keyup.enter="submit"
        />
      </div>

      <div v-if="hasGroups" class="form-row">
        <label class="form-label">{{ t('addSource.group') }}</label>
        <AppleSelect
          v-model="groupId"
          class="apple-select"
          :options="groupOptions"
        />
      </div>

      <div v-if="error" class="error-banner">
        <Icon name="info" :size="14" />
        <span>{{ error }}</span>
      </div>
    </div>

    <template #footer>
      <button class="f-btn" @click="emit('close')">{{ t('addSource.cancel') }}</button>
      <button class="f-btn primary" :disabled="adding || !url.trim()" @click="submit">
        <Icon v-if="adding" name="arrow-clockwise" :size="14" class="spin" />
        <span>{{ adding ? t('addSource.adding') : t('addSource.submit') }}</span>
      </button>
    </template>
  </Modal>
</template>

<style scoped>
.form-container {
  display: flex;
  flex-direction: column;
  gap: 0.85rem;
  padding: 0.3rem 0;
}

.form-row {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}

.form-label {
  font-size: 0.82rem;
  font-weight: 550;
  color: var(--text-secondary);
}

.apple-input,
.apple-select {
  width: 100%;
  max-width: 100%;
}

.error-banner {
  padding: 0.6rem 0.9rem;
  border-radius: 8px;
  background: var(--danger-tint);
  color: var(--danger);
  font-size: 0.8rem;
  display: flex;
  align-items: center;
  gap: 0.45rem;
  word-break: break-all;
}
</style>
