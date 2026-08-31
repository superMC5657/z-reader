<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { openUrl } from '@tauri-apps/plugin-opener'
import { useDataStore } from '../../stores/data'
import { useAppStore } from '../../stores/app'
import { fetchFullContent } from '../../lib/tauri'
import { formatFullTime } from '../../lib/time'

const { t } = useI18n()
const data = useDataStore()
const app = useAppStore()
const fetchingFull = ref(false)
const extractError = ref('')

const item = computed(() => data.selectedItem)
const source = computed(() => (item.value ? data.sourceById(item.value.sourceId) : undefined))

// Match the reader iframe's palette to the app theme (the iframe can't inherit CSS vars).
const isDark = computed(() => {
  const theme = app.s.theme
  return theme === 'dark' || (theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches)
})

const docHtml = computed(() => {
  if (!item.value?.content) return ''
  const fg = isDark.value ? '#f5f5f7' : '#1d1d1f'
  const muted = isDark.value ? '#a1a1a6' : '#6e6e73'
  const border = isDark.value ? 'rgba(255,255,255,0.16)' : 'rgba(0,0,0,0.12)'
  const chip = isDark.value ? 'rgba(120,120,128,0.32)' : 'rgba(120,120,128,0.14)'
  return `<!doctype html><html><head><meta charset="utf-8"><base target="_blank"><style>
    body { font-family: 'Inter Variable', -apple-system, 'SF Pro Text', 'Segoe UI', 'PingFang SC', 'Microsoft YaHei UI', system-ui, sans-serif;
           letter-spacing: -0.01em; color: ${fg}; line-height: 1.8; padding: 0 1.5rem 2.5rem;
           max-width: 40rem; margin: 0 auto; font-size: 1rem; }
    h1, h2, h3 { letter-spacing: -0.02em; line-height: 1.3; margin: 1.6em 0 0.5em; }
    img, video { max-width: 100%; height: auto; border-radius: 10px; margin: 0.5rem 0; }
    pre { overflow-x: auto; background: ${chip}; padding: 0.9rem 1.1rem; border-radius: 10px; font-size: 0.85rem; line-height: 1.6; }
    code { font-family: 'SF Mono', Consolas, 'JetBrains Mono', monospace; }
    p > code, li > code { background: ${chip}; padding: 0.1em 0.35em; border-radius: 5px; font-size: 0.85em; }
    blockquote { border-left: 3px solid #0a84ff; margin: 0.8rem 0; padding: 0.1rem 1.1rem; color: ${muted}; }
    a { color: #0a84ff; text-decoration: none; }
    a:hover { text-decoration: underline; }
    table { border-collapse: collapse; width: 100%; } td, th { border: 1px solid ${border}; padding: 0.35rem 0.7rem; }
    hr { border: none; border-top: 0.5px solid ${border}; margin: 1.6rem 0; }
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
  padding: 1rem 1.6rem 0.8rem;
  border-bottom: 0.5px solid var(--border);
  background: var(--bg-card);
}

.head-top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.8rem;
}

.head-meta {
  font-size: 0.78rem;
  color: var(--text-secondary);
  display: flex;
  align-items: center;
  gap: 0.35rem;
  min-width: 0;
  overflow: hidden;
  flex-wrap: wrap;
  font-variant-numeric: tabular-nums;
}

.source {
  color: var(--accent);
  font-weight: 600;
  flex-shrink: 0;
}

.dot {
  color: var(--text-tertiary);
}

.actions {
  display: flex;
  gap: 0.1rem;
  flex-shrink: 0;
}

.actions .active {
  color: var(--star);
}

.title {
  font-size: 1.65rem;
  font-weight: 800;
  line-height: 1.25;
  letter-spacing: -0.025em;
  margin-top: 0.55rem;
  max-width: 40rem;
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
  padding: 0.5rem 1.6rem;
  font-size: 0.8rem;
  color: var(--danger);
}
</style>
