<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import Modal from './Modal.vue'
import Icon from './Icon.vue'

withDefaults(
  defineProps<{
    title?: string
    message?: string
    confirmText?: string
    cancelText?: string
    danger?: boolean
    loading?: boolean
  }>(),
  {
    title: '',
    message: '',
    confirmText: '',
    cancelText: '',
    danger: false,
    loading: false,
  }
)

const emit = defineEmits<{
  confirm: []
  cancel: []
  close: []
}>()

const { t } = useI18n()

function handleClose() {
  emit('cancel')
  emit('close')
}

function handleConfirm() {
  emit('confirm')
}
</script>

<template>
  <Modal :title="title" @close="handleClose">
    <div class="confirm-content">
      <slot>
        <p class="confirm-message">{{ message }}</p>
      </slot>
    </div>
    <template #footer>
      <button class="f-btn" :disabled="loading" @click="handleClose">
        {{ cancelText || t('common.cancel') }}
      </button>
      <button
        class="f-btn"
        :class="danger ? 'danger' : 'primary'"
        :disabled="loading"
        @click="handleConfirm"
      >
        <Icon v-if="loading" name="refresh" :size="13" class="spin" />
        <span>{{ confirmText || t('common.confirm') }}</span>
      </button>
    </template>
  </Modal>
</template>

<style scoped>
.confirm-content {
  display: flex;
  flex-direction: column;
}

.confirm-message {
  font-size: 13px;
  color: var(--text-secondary);
  line-height: 1.55;
  margin: 0;
  word-break: break-word;
}
</style>
