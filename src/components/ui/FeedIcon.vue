<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { convertFileSrc } from '@tauri-apps/api/core'
import type { Source } from '../../types'

const props = withDefaults(
  defineProps<{
    source: Source
    size?: number
  }>(),
  {
    size: 16,
  }
)

// 0 = local file, 1 = Google Favicon, 2 = DuckDuckGo Favicon, 3 = Initial Badge
const stage = ref(props.source.favicon ? 0 : 1)

watch(
  () => [props.source.favicon, props.source.url],
  () => {
    stage.value = props.source.favicon ? 0 : 1
  }
)

function getDomain(urlStr: string): string {
  try {
    return new URL(urlStr).hostname
  } catch {
    return ''
  }
}

const domain = computed(() => getDomain(props.source.url))

const currentSrc = computed(() => {
  if (stage.value === 0 && props.source.favicon) {
    return convertFileSrc(props.source.favicon)
  }
  if (stage.value <= 1 && domain.value) {
    return `https://www.google.com/s2/favicons?domain=${domain.value}&sz=64`
  }
  if (stage.value <= 2 && domain.value) {
    return `https://icons.duckduckgo.com/ip2/${domain.value}.ico`
  }
  return null
})

function onError() {
  if (stage.value < 3) {
    stage.value++
  }
}

const initial = computed(() => {
  const trimmed = props.source.title.trim()
  if (!trimmed) return 'R'
  const ch = Array.from(trimmed)[0]
  return ch.toUpperCase()
})

const PALETTES = [
  '#007aff',
  '#34c759',
  '#ff9500',
  '#af52de',
  '#ff2d55',
  '#5856d6',
  '#00c7be',
  '#ff3b30',
  '#30b0c7',
  '#64d2ff',
]

const initialBg = computed(() => {
  let hash = 0
  const title = props.source.title
  for (let i = 0; i < title.length; i++) {
    hash = (hash << 5) - hash + title.charCodeAt(i)
    hash |= 0
  }
  return PALETTES[Math.abs(hash) % PALETTES.length]
})
</script>

<template>
  <img
    v-if="currentSrc && stage < 3"
    class="feed-favicon"
    :src="currentSrc"
    alt=""
    loading="lazy"
    :style="{ width: `${size}px`, height: `${size}px` }"
    @error="onError"
  />
  <span
    v-else
    class="feed-favicon initial-badge"
    :style="{
      width: `${size}px`,
      height: `${size}px`,
      fontSize: `${Math.max(9, Math.round(size * 0.58))}px`,
      background: initialBg,
    }"
  >
    {{ initial }}
  </span>
</template>

<style scoped>
.feed-favicon {
  border-radius: 4px;
  flex-shrink: 0;
  object-fit: cover;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  vertical-align: middle;
}

.initial-badge {
  font-weight: 750;
  color: #ffffff;
  line-height: 1;
  text-transform: uppercase;
  letter-spacing: -0.02em;
  box-shadow: inset 0 0 0 0.5px rgba(255, 255, 255, 0.25);
  user-select: none;
}
</style>
