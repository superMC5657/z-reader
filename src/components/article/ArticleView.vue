<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { openUrl } from '@tauri-apps/plugin-opener'
import { useDataStore } from '../../stores/data'
import { fetchFullContent } from '../../lib/tauri'
import { formatFullTime } from '../../lib/time'

const { t } = useI18n()
const data = useDataStore()
const fetchingFull = ref(false)
const extractError = ref('')

const item = computed(() => data.selectedItem)
const source = computed(() => (item.value ? data.sourceById(item.value.sourceId) : undefined))

const docHtml = computed(() => {
  if (!item.value?.content) return ''
  return `<!doctype html><html><head><meta charset="utf-8"><base target="_blank"><style>
    body { font-family: 'Segoe UI Variable', 'Segoe UI', 'Microsoft YaHei UI', 'PingFang SC', system-ui, sans-serif;
           color: var(--text-primary); line-height: 1.75; padding: 0 1.5rem 2rem; max-width: 46rem; margin: 0 auto; }
    img, video { max-width: 100%; height: auto; border-radius: 4px; }
    pre { overflow-x: auto; background: rgba(127,127,127,0.12); padding: 0.8rem; border-radius: 6px; }
    code { font-family: Consolas, 'JetBrains Mono', monospace; }
    blockquote { border-left: 3px solid var(--accent); margin: 0.6rem 0; padding: 0.1rem 1rem; color: var(--text-secondary); }
    a { color: var(--accent); }
    table { border-collapse: collapse; } td, th { border: 1px solid var(--border); padding: 0.3rem 0.6rem; }
  </style></head><body>${item.value.content}</body></html>`
})

watch(item, () => {
  extractError.value = ''
})

async function onFetchFull() {
  if (!item.value) return
  fetchingFull.value = true
  extractError.value = ''
  try {
    await fetchFullContent(item.value.id)
    await data.selectItem(item.value.id)
    // reload the freshly stored item in the list too
    data.loadItems().catch(() => {})
  } catch (e) {
    extractError.value = String(e)
  } finally {
    fetchingFull.value = false
  }
}

function onIframeLoad(e: Event) {
  // sandboxed with allow-same-origin (no allow-scripts): intercept link clicks
  const doc = (e.target as HTMLIFrameElement).contentDocument
  if (!doc) return
  doc.addEventListener('click', (ev) => {
    const anchor = (ev.target as HTMLElement).closest('a')
    if (anchor?.href) {
      ev.preventDefault()
      openUrl(anchor.href)
    }
  })
}
</script>

<template>
  <section v-if="item" class="article-view">
    <header class="head">
      <div class="head-top">
        <div class="head-meta">
          <span class="source">{{ source?.title }}</span>
          <span class="dot">·</span>
          <span>{{ formatFullTime(item.publishedAt) }}</span>
          <span v-if="item.author" class="dot">·</span>
          <span v-if="item.author">{{ item.author }}</span>
        </div>
        <div class="actions">
          <button
            class="f-icon-btn"
            :title="t('item.fetchFull')"
            :disabled="fetchingFull"
            @click="onFetchFull"
          >
            <span :class="{ spin: fetchingFull }">⟳</span>
          </button>
          <button
            v-if="item.url"
            class="f-icon-btn"
            :title="t('item.openWeb')"
            @click="openUrl(item.url)"
          >
            ↗
          </button>
          <button
            class="f-icon-btn"
            :class="{ active: item.starred }"
            :title="t(item.starred ? 'item.unstar' : 'item.star')"
            @click="data.toggleStar(item)"
          >
            {{ item.starred ? '★' : '☆' }}
          </button>
          <button
            class="f-icon-btn"
            :title="t('item.markUnread')"
            @click="data.setItemRead(item, false)"
          >
            ○
          </button>
        </div>
      </div>
      <h1 class="title">{{ item.title }}</h1>
    </header>

    <div v-if="extractError" class="error">{{ t('common.error') }}: {{ extractError }}</div>

    <iframe
      v-if="item.content"
      class="content"
      sandbox="allow-same-origin"
      :srcdoc="docHtml"
      @load="onIframeLoad"
    ></iframe>
    <div v-else class="state">
      <p>{{ t('item.noContent') }}</p>
      <p class="sub">{{ t('item.noContentHint') }}</p>
      <button class="f-btn" :disabled="fetchingFull" @click="onFetchFull">
        {{ fetchingFull ? t('item.fetching') : t('item.fetchFull') }}
      </button>
    </div>
  </section>
</template>

<style scoped>
.article-view {
  height: 100%;
  display: flex;
  flex-direction: column;
  min-width: 0;
  background: var(--bg-card);
}

.head {
  padding: 0.9rem 1.5rem 0.6rem;
  border-bottom: 1px solid var(--border);
}

.head-top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.8rem;
}

.head-meta {
  font-size: 0.8rem;
  color: var(--text-secondary);
  display: flex;
  align-items: center;
  gap: 0.35rem;
  min-width: 0;
  overflow: hidden;
}

.source {
  color: var(--accent);
  flex-shrink: 0;
}

.dot {
  color: var(--text-tertiary);
}

.actions {
  display: flex;
  gap: 0.15rem;
  flex-shrink: 0;
}

.actions .active {
  color: #f2b705;
}

.title {
  font-size: 1.35rem;
  font-weight: 700;
  line-height: 1.4;
  margin-top: 0.5rem;
}

.content {
  flex: 1;
  border: none;
  width: 100%;
  user-select: text;
}

.state {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 0.5rem;
  color: var(--text-secondary);
}

.state .sub {
  font-size: 0.82rem;
  color: var(--text-tertiary);
}

.error {
  padding: 0.5rem 1.5rem;
  font-size: 0.8rem;
  color: var(--danger);
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.spin {
  display: inline-block;
  animation: spin 1s linear infinite;
}
</style>
