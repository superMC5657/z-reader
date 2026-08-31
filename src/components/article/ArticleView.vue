<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { openUrl } from '@tauri-apps/plugin-opener'
import { useDataStore } from '../../stores/data'
import { useAppStore } from '../../stores/app'
import { fetchFullContent } from '../../lib/tauri'
import { formatFullTime } from '../../lib/time'
import Icon from '../ui/Icon.vue'

defineProps<{
  isFocusModal?: boolean
}>()

const { t } = useI18n()
const data = useDataStore()
const app = useAppStore()
const fetchingFull = ref(false)
const extractError = ref('')

const item = computed(() => data.selectedItem)
const source = computed(() => (item.value ? data.sourceById(item.value.sourceId) : undefined))

// Match the reader iframe's palette to the app theme
const isDark = computed(() => app.isDark)

const docHtml = computed(() => {
  if (!item.value?.content) return ''
  const fg = isDark.value ? '#f5f5f7' : '#1d1d1f'
  const bg = isDark.value ? '#2c2c2e' : '#ffffff'
  const muted = isDark.value ? '#a1a1a6' : '#6e6e73'
  const border = isDark.value ? 'rgba(255,255,255,0.12)' : 'rgba(0,0,0,0.08)'
  const chip = isDark.value ? 'rgba(120,120,128,0.25)' : 'rgba(120,120,128,0.12)'
  const codeBg = isDark.value ? '#1e1e20' : '#f2f2f7'
  const accent = isDark.value ? '#0a84ff' : '#007aff'

  return `<!doctype html><html data-theme="${isDark.value ? 'dark' : 'light'}"><head><meta charset="utf-8"><meta name="color-scheme" content="${isDark.value ? 'dark' : 'light'}"><base target="_blank"><style>
    :root {
      color-scheme: ${isDark.value ? 'dark' : 'light'};
    }
    * {
      box-sizing: border-box;
    }
    html {
      background-color: ${bg};
      color: ${fg};
    }
    body {
      font-family: -apple-system, BlinkMacSystemFont, 'SF Pro Text', 'Inter Variable', 'PingFang SC', 'Hiragino Sans GB', 'Microsoft YaHei', system-ui, sans-serif;
      letter-spacing: -0.015em;
      background-color: ${bg};
      color: ${fg};
      line-height: 1.82;
      padding: 0 2rem 3.5rem;
      max-width: 42rem;
      margin: 0 auto;
      font-size: 1.05rem;
      -webkit-font-smoothing: antialiased;
      word-break: break-word;
    }
    h1, h2, h3, h4, h5, h6 {
      letter-spacing: -0.025em;
      line-height: 1.35;
      margin: 1.8em 0 0.6em;
      font-weight: 700;
      color: ${fg};
    }
    h1 { font-size: 1.65rem; }
    h2 { font-size: 1.35rem; }
    h3 { font-size: 1.15rem; }
    p, li, span, div, font {
      color: inherit;
    }
    p { margin: 1em 0; }
    img, video {
      max-width: 100%;
      height: auto;
      border-radius: 12px;
      margin: 1rem 0;
      box-shadow: 0 2px 10px rgba(0,0,0,${isDark.value ? '0.35' : '0.08'});
      display: block;
    }
    ${isDark.value ? `
      img[src$=".svg"], img[src*=".svg?"] {
        background: rgba(255, 255, 255, 0.06);
        padding: 4px;
      }
      [style*="background-color: rgb(255"],
      [style*="background-color:#fff"],
      [style*="background-color: #fff"],
      [style*="background-color: white"],
      [style*="background: rgb(255"],
      [style*="background: #fff"],
      [style*="background: white"] {
        background-color: transparent !important;
      }
      [style*="color: rgb(0"],
      [style*="color: rgb(34"],
      [style*="color: rgb(51"],
      [style*="color: #0"],
      [style*="color: #1"],
      [style*="color: #2"],
      [style*="color: #3"],
      [style*="color: black"] {
        color: inherit !important;
      }
    ` : ''}
    pre {
      overflow-x: auto;
      background: ${codeBg};
      border: 1px solid ${border};
      padding: 1rem 1.25rem;
      border-radius: 12px;
      font-size: 0.88rem;
      line-height: 1.6;
      margin: 1.2rem 0;
      color: ${fg};
    }
    code {
      font-family: 'SF Mono', ui-monospace, Menlo, Monaco, Consolas, monospace;
    }
    p > code, li > code {
      background: ${chip};
      padding: 0.15em 0.4em;
      border-radius: 6px;
      font-size: 0.88em;
      color: ${fg};
    }
    blockquote {
      border-left: 3.5px solid ${accent};
      margin: 1.2rem 0;
      padding: 0.2rem 1.2rem;
      color: ${muted};
      font-style: normal;
      background: ${isDark.value ? 'rgba(255,255,255,0.04)' : 'rgba(0,0,0,0.02)'};
      border-radius: 0 8px 8px 0;
    }
    a { color: ${accent}; text-decoration: none; }
    a:hover { text-decoration: underline; }
    table { border-collapse: collapse; width: 100%; margin: 1.2rem 0; color: ${fg}; }
    td, th { border: 1px solid ${border}; padding: 0.45rem 0.85rem; text-align: left; }
    th { background: ${chip}; font-weight: 600; }
    hr { border: none; border-top: 0.5px solid ${border}; margin: 2rem 0; }
    ul, ol { padding-left: 1.6rem; margin: 0.8rem 0; }
    li { margin: 0.35rem 0; }
    ::-webkit-scrollbar { width: 8px; height: 8px; }
    ::-webkit-scrollbar-thumb { background: rgba(120, 120, 128, 0.3); border-radius: 10px; }
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
    data.loadItems().catch(() => {})
  } catch (e) {
    extractError.value = String(e)
  } finally {
    fetchingFull.value = false
  }
}

function onIframeLoad(e: Event) {
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
    <!-- Safari Reader Header -->
    <header class="reader-head" data-tauri-drag-region>
      <div class="head-top" data-tauri-drag-region>
        <div class="head-meta">
          <button
            class="f-icon-btn mobile-back-btn"
            title="Back"
            @click="data.selectedItem = null; data.selectedId = null"
          >
            <Icon name="chevron-right" :size="15" style="transform: rotate(180deg)" />
          </button>
          <span class="source-tag">{{ source?.title }}</span>
          <span class="dot">·</span>
          <span class="time">{{ formatFullTime(item.publishedAt) }}</span>
          <template v-if="item.author">
            <span class="dot">·</span>
            <span class="author">{{ item.author }}</span>
          </template>
        </div>

        <div class="action-group">
          <!-- Fetch Full Text -->
          <button
            class="f-icon-btn reader-action-btn"
            :title="t('item.fetchFull')"
            :disabled="fetchingFull"
            @click="onFetchFull"
          >
            <Icon
              :name="fetchingFull ? 'arrow-clockwise' : 'sparkles'"
              :size="15"
              :class="{ spin: fetchingFull }"
            />
          </button>

          <!-- Open in Browser -->
          <button
            v-if="item.url"
            class="f-icon-btn reader-action-btn"
            :title="t('item.openWeb')"
            @click="openUrl(item.url)"
          >
            <Icon name="open-web" :size="15" />
          </button>

          <!-- Star -->
          <button
            class="f-icon-btn reader-action-btn"
            :class="{ 'active-star': item.starred }"
            :title="t(item.starred ? 'item.unstar' : 'item.star')"
            @click="data.toggleStar(item)"
          >
            <Icon :name="item.starred ? 'star-fill' : 'star'" :size="15" />
          </button>

          <!-- Mark Unread -->
          <button
            class="f-icon-btn reader-action-btn"
            :title="t('item.markUnread')"
            @click="data.setItemRead(item, false)"
          >
            <Icon name="circle" :size="15" />
          </button>

          <!-- Toggle Focus Mode -->
          <button
            class="f-icon-btn reader-action-btn focus-toggle-btn"
            :class="{ 'active-focus': app.isFocusMode }"
            :title="t(app.isFocusMode ? 'item.splitMode' : 'item.focusMode')"
            @click="app.toggleFocusMode()"
          >
            <Icon :name="app.isFocusMode ? 'split' : 'focus'" :size="14.5" />
          </button>

          <!-- Explicit Close Button -->
          <button
            class="f-icon-btn reader-action-btn close-article-btn"
            :title="t('item.close')"
            @click="data.selectedItem = null; data.selectedId = null"
          >
            <Icon name="close" :size="13" />
          </button>
        </div>
      </div>

      <h1 class="title">{{ item.title }}</h1>
    </header>

    <div v-if="extractError" class="error-banner">
      <Icon name="info" :size="15" />
      <span>{{ t('common.error') }}: {{ extractError }}</span>
    </div>

    <!-- Reader Iframe -->
    <iframe
      v-if="item.content"
      class="reader-frame"
      sandbox="allow-same-origin"
      :srcdoc="docHtml"
      @load="onIframeLoad"
    ></iframe>

    <!-- No Content Empty State -->
    <div v-else class="empty-reader">
      <div class="empty-icon-circle">
        <Icon name="sparkles" :size="32" color="var(--text-quaternary)" />
      </div>
      <p class="empty-title">{{ t('item.noContent') }}</p>
      <p class="empty-sub">{{ t('item.noContentHint') }}</p>
      <button class="f-btn primary" :disabled="fetchingFull" @click="onFetchFull">
        <Icon name="arrow-clockwise" :size="14" :class="{ spin: fetchingFull }" />
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

.reader-head {
  padding: 1.1rem 1.8rem 1rem;
  border-bottom: 0.5px solid var(--border);
  background: var(--bg-card);
  backdrop-filter: var(--glass-blur);
  -webkit-backdrop-filter: var(--glass-blur);
}

.head-top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
}

.head-meta {
  font-size: 0.8rem;
  color: var(--text-secondary);
  display: flex;
  align-items: center;
  gap: 0.4rem;
  min-width: 0;
  overflow: hidden;
  flex-wrap: wrap;
  font-variant-numeric: tabular-nums;
}

.source-tag {
  color: var(--accent);
  font-weight: 600;
  flex-shrink: 0;
}

.dot {
  color: var(--text-quaternary);
}

.author {
  color: var(--text-secondary);
}

.action-group {
  display: flex;
  gap: 0.2rem;
  align-items: center;
  flex-shrink: 0;
}

.reader-action-btn {
  border-radius: 8px;
}

.reader-action-btn:hover {
  background: var(--bg-hover-strong);
}

.reader-action-btn.active-focus {
  color: var(--accent);
  background: var(--accent-tint);
}

.reader-action-btn.close-article-btn:hover {
  background: var(--bg-hover-strong);
  color: var(--danger);
}

.title {
  font-size: 1.7rem;
  font-weight: 750;
  line-height: 1.28;
  letter-spacing: -0.03em;
  margin-top: 0.7rem;
  max-width: 44rem;
  color: var(--text-primary);
}

.reader-frame {
  flex: 1;
  border: none;
  width: 100%;
  user-select: text;
  background: transparent;
}

.error-banner {
  padding: 0.65rem 1.8rem;
  font-size: 0.82rem;
  color: var(--danger);
  background: var(--danger-tint);
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.empty-reader {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 0.55rem;
  color: var(--text-secondary);
  padding: 2rem;
}

.empty-icon-circle {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 4.2rem;
  height: 4.2rem;
  border-radius: 50%;
  background: var(--bg-track);
  margin-bottom: 0.4rem;
}

.empty-title {
  font-size: 1.05rem;
  font-weight: 600;
  color: var(--text-primary);
}

.mobile-back-btn {
  display: none;
  width: 1.8rem;
  height: 1.8rem;
  margin-right: 0.2rem;
}

@media (max-width: 980px) {
  .mobile-back-btn {
    display: inline-flex;
  }
}
</style>
