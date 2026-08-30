<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useDataStore } from '../stores/data'
import Modal from './ui/Modal.vue'

const { t } = useI18n()
const data = useDataStore()

const emit = defineEmits<{ close: []; added: [] }>()

const url = ref('')
const groupId = ref<string>('')
const adding = ref(false)
const error = ref('')

const hasGroups = computed(() => data.groups.length > 0)

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
    <div class="form-row">
      <label>{{ t('addSource.urlLabel') }}</label>
      <input
        v-model="url"
        :placeholder="t('addSource.urlPlaceholder')"
        autofocus
        @keyup.enter="submit"
      />
    </div>
    <div v-if="hasGroups" class="form-row">
      <label>{{ t('addSource.group') }}</label>
      <select v-model="groupId">
        <option value="">{{ t('addSource.none') }}</option>
        <option v-for="g in data.groups" :key="g.id" :value="String(g.id)">{{ g.name }}</option>
      </select>
    </div>
    <div v-if="error" class="error">{{ error }}</div>
    <template #footer>
      <button class="f-btn" @click="emit('close')">{{ t('addSource.cancel') }}</button>
      <button class="f-btn primary" :disabled="adding || !url.trim()" @click="submit">
        {{ adding ? t('addSource.adding') : t('addSource.submit') }}
      </button>
    </template>
  </Modal>
</template>

<style scoped>
.error {
  color: var(--danger);
  font-size: 0.8rem;
  margin-top: 0.4rem;
  word-break: break-all;
}
</style>
