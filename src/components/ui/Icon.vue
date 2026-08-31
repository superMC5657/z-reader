<script setup lang="ts">
import { computed } from 'vue'

const props = withDefaults(
  defineProps<{
    name: string
    size?: number | string
    color?: string
    strokeWidth?: number | string
  }>(),
  {
    size: 16,
    color: 'currentColor',
    strokeWidth: 2,
  },
)

const pixelSize = computed(() => {
  return typeof props.size === 'number' ? `${props.size}px` : props.size
})
</script>

<template>
  <svg
    class="sf-icon"
    :style="{ width: pixelSize, height: pixelSize, color: props.color }"
    viewBox="0 0 24 24"
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
  >
    <!-- All Feeds / Tray Stack -->
    <template v-if="name === 'all' || name === 'tray-stack'">
      <path
        d="M4 6.5C4 5.67157 4.67157 5 5.5 5H18.5C19.3284 5 20 5.67157 20 6.5V17.5C20 18.3284 19.3284 19 18.5 19H5.5C4.67157 19 4 18.3284 4 17.5V6.5Z"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
      <path
        d="M4 13.5H8.2C8.69706 13.5 9.17374 13.6976 9.52513 14.049C9.87652 14.4004 10.074 14.8771 10.074 15.3741C10.074 15.7231 10.357 16.006 10.706 16.006H13.294C13.643 16.006 13.926 15.7231 13.926 15.3741C13.926 14.8771 14.1235 14.4004 14.4749 14.049C14.8263 13.6976 15.3029 13.5 15.8 13.5H20"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
      <path
        d="M8 9H16"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
      />
    </template>

    <!-- Folder -->
    <template v-else-if="name === 'folder'">
      <path
        d="M3.75 6.75C3.75 5.50736 4.75736 4.5 6 4.5H9.41421C9.81205 4.5 10.1936 4.65804 10.4749 4.93934L12.0607 6.52513C12.342 6.80643 12.7235 6.96447 13.1213 6.96447H18C19.2426 6.96447 20.25 7.97183 20.25 9.21447V17.25C20.25 18.4926 19.2426 19.5 18 19.5H6C4.75736 19.5 3.75 18.4926 3.75 17.25V6.75Z"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linejoin="round"
      />
    </template>

    <!-- Folder Plus -->
    <template v-else-if="name === 'folder-plus'">
      <path
        d="M3.75 6.75C3.75 5.50736 4.75736 4.5 6 4.5H9.41421C9.81205 4.5 10.1936 4.65804 10.4749 4.93934L12.0607 6.52513C12.342 6.80643 12.7235 6.96447 13.1213 6.96447H18C19.2426 6.96447 20.25 7.97183 20.25 9.21447V17.25C20.25 18.4926 19.2426 19.5 18 19.5H6C4.75736 19.5 3.75 18.4926 3.75 17.25V6.75Z"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linejoin="round"
      />
      <path
        d="M12 11.5V15.5M10 13.5H14"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </template>

    <!-- Chevron Left -->
    <template v-else-if="name === 'chevron-left'">
      <path
        d="M14.5 7L9.5 12L14.5 17"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </template>

    <!-- Chevron Right -->
    <template v-else-if="name === 'chevron-right'">
      <path
        d="M9.5 7L14.5 12L9.5 17"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </template>

    <!-- Chevron Down -->
    <template v-else-if="name === 'chevron-down'">
      <path
        d="M7 9.5L12 14.5L17 9.5"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </template>

    <!-- Chevrons Up Down (macOS PopUp button indicator) -->
    <template v-else-if="name === 'chevrons-up-down' || name === 'chevron-up-down'">
      <path
        d="M7 15L12 20L17 15M7 9L12 4L17 9"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </template>

    <!-- Sun (Light Theme) -->
    <template v-else-if="name === 'sun'">
      <circle cx="12" cy="12" r="4" :stroke="props.color" :stroke-width="props.strokeWidth" />
      <path
        d="M12 2V4M12 20V22M4 12H2M22 12H20M19.07 4.93L17.66 6.34M6.34 17.66L4.93 19.07M19.07 19.07L17.66 17.66M6.34 6.34L4.93 4.93"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
      />
    </template>

    <!-- Moon (Dark Theme) -->
    <template v-else-if="name === 'moon'">
      <path
        d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79Z"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </template>

    <!-- Display (System Theme) -->
    <template v-else-if="name === 'display' || name === 'desktop'">
      <rect
        x="2"
        y="3"
        width="20"
        height="14"
        rx="2"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
      />
      <path
        d="M8 21H16M12 17V21"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
      />
    </template>

    <!-- Globe (Language) -->
    <template v-else-if="name === 'globe'">
      <circle
        cx="12"
        cy="12"
        r="9"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
      />
      <path
        d="M3.6 9H20.4M3.6 15H20.4M12 3C14.5 5.5 15.8 8.6 15.8 12C15.8 15.4 14.5 18.5 12 21C9.5 18.5 8.2 15.4 8.2 12C8.2 8.6 9.5 5.5 12 3Z"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </template>

    <!-- Plus -->
    <template v-else-if="name === 'plus'">
      <path
        d="M12 5V19M5 12H19"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </template>

    <!-- Refresh / Clockwise -->
    <template v-else-if="name === 'refresh' || name === 'arrow-clockwise'">
      <path
        d="M20 11C19.5539 7.05369 16.2098 4 12.11 4C7.63 4 4 7.58 4 12C4 16.42 7.63 20 12.11 20C15.82 20 18.91 17.51 19.82 14.1"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
      <path
        d="M20 4V11H13"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </template>

    <!-- Settings / Gear -->
    <template v-else-if="name === 'gear' || name === 'settings'">
      <path
        d="M12 15C13.6569 15 15 13.6569 15 12C15 10.3431 13.6569 9 12 9C10.3431 9 9 10.3431 9 12C9 13.6569 10.3431 15 12 15Z"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
      <path
        d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </template>

    <!-- Search / Magnifying Glass -->
    <template v-else-if="name === 'search' || name === 'magnifying-glass'">
      <path
        d="M10.5 18C14.6421 18 18 14.6421 18 10.5C18 6.35786 14.6421 3 10.5 3C6.35786 3 3 6.35786 3 10.5C3 14.6421 6.35786 18 10.5 18Z"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
      <path
        d="M16 16L21 21"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </template>

    <!-- Clear / X Mark / Close -->
    <template v-else-if="name === 'close' || name === 'xmark' || name === 'x'">
      <path
        d="M6 6L18 18M18 6L6 18"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </template>

    <!-- Focus Mode (Expand to Center Focus Modal) -->
    <template v-else-if="name === 'focus' || name === 'expand'">
      <path
        d="M15 3H21V9M9 21H3V15M21 3L14 10M3 21L10 14"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </template>

    <!-- Split Mode / Columns / Exit Focus -->
    <template v-else-if="name === 'split' || name === 'collapse'">
      <path
        d="M4 14H10V20M20 10H14V4M10 14L3 21M14 10L21 3"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </template>

    <!-- Reader Layout / Split View -->
    <template v-else-if="name === 'layout-split' || name === 'sidebar-right'">
      <rect x="3" y="4" width="18" height="16" rx="3" :stroke="props.color" :stroke-width="props.strokeWidth" />
      <path d="M14 4V20" :stroke="props.color" :stroke-width="props.strokeWidth" />
    </template>

    <!-- Checkmark -->
    <template v-else-if="name === 'checkmark'">
      <path
        d="M5 12.5L9.5 17L19 7.5"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </template>

    <!-- Checkmark Circle / Mark All Read -->
    <template v-else-if="name === 'checkmark-circle' || name === 'mark-all-read'">
      <circle
        cx="12"
        cy="12"
        r="9"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
      />
      <path
        d="M8 12L10.8 14.8L16 9.5"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </template>

    <!-- Star Fill -->
    <template v-else-if="name === 'star-fill'">
      <path
        d="M12 2.5L15.09 8.76L22 9.77L17 14.64L18.18 21.52L12 18.27L5.82 21.52L7 14.64L2 9.77L8.91 8.76L12 2.5Z"
        fill="currentColor"
        stroke="currentColor"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </template>

    <!-- Star Outline -->
    <template v-else-if="name === 'star'">
      <path
        d="M12 2.5L15.09 8.76L22 9.77L17 14.64L18.18 21.52L12 18.27L5.82 21.52L7 14.64L2 9.77L8.91 8.76L12 2.5Z"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </template>

    <!-- View: Cards -->
    <template v-else-if="name === 'view-cards'">
      <rect
        x="3.5"
        y="4"
        width="7.5"
        height="6.8"
        rx="2"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
      />
      <rect
        x="13"
        y="4"
        width="7.5"
        height="6.8"
        rx="2"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
      />
      <rect
        x="3.5"
        y="13.2"
        width="7.5"
        height="6.8"
        rx="2"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
      />
      <rect
        x="13"
        y="13.2"
        width="7.5"
        height="6.8"
        rx="2"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
      />
    </template>

    <!-- View: List -->
    <template v-else-if="name === 'view-list'">
      <path
        d="M4 6H20M4 12H20M4 18H20"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
      />
    </template>

    <!-- View: Magazine -->
    <template v-else-if="name === 'view-magazine'">
      <rect
        x="3.5"
        y="4"
        width="6"
        height="6.5"
        rx="1.5"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
      />
      <path
        d="M12.5 5.5H20M12.5 9H17.5"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
      />
      <rect
        x="3.5"
        y="13.5"
        width="6"
        height="6.5"
        rx="1.5"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
      />
      <path
        d="M12.5 15H20M12.5 18.5H17.5"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
      />
    </template>

    <!-- View: Compact -->
    <template v-else-if="name === 'view-compact'">
      <path
        d="M4 5H20M4 9H20M4 13H20M4 17H20M4 21H20"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
      />
    </template>

    <!-- Arrow Up Right / Open in Browser -->
    <template v-else-if="name === 'open-web' || name === 'arrow-up-right'">
      <path
        d="M18 13V18C18 19.1046 17.1046 20 16 20H6C4.89543 20 4 19.1046 4 18V8C4 6.89543 4.89543 6 6 6H11"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
      />
      <path
        d="M15 4H20V9"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
      <path
        d="M10 14L20 4"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
      />
    </template>

    <!-- Fetch Full Content / Sparkles -->
    <template v-else-if="name === 'sparkles' || name === 'doc-text-magnifyingglass'">
      <path
        d="M14 2H6C4.89543 2 4 2.89543 4 4V20C4 21.1046 4.89543 22 6 22H18C19.1046 22 20 21.1046 20 20V8L14 2Z"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
      <path
        d="M14 2V8H20"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
      <path
        d="M8 13H16M8 17H13"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
      />
    </template>

    <!-- Circle (Unread Toggle) -->
    <template v-else-if="name === 'circle'">
      <circle
        cx="12"
        cy="12"
        r="8"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
      />
    </template>

    <!-- Circle Fill -->
    <template v-else-if="name === 'circle-fill'">
      <circle
        cx="12"
        cy="12"
        r="7.5"
        fill="currentColor"
      />
    </template>

    <!-- Trash / Delete -->
    <template v-else-if="name === 'trash' || name === 'delete'">
      <path
        d="M4 7H20M10 11V17M14 11V17M5 7L6 19C6 20.1046 6.89543 21 8 21H16C17.1046 21 18 20.1046 18 19L19 7M9 7V4C9 3.44772 9.44772 3 10 3H14C14.5523 3 15 3.44772 15 4V7"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </template>

    <!-- Pencil / Edit / Rename -->
    <template v-else-if="name === 'pencil' || name === 'rename'">
      <path
        d="M17 3L21 7L7 21H3V17L17 3Z"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </template>

    <!-- Import / Download -->
    <template v-else-if="name === 'import' || name === 'arrow-down-tray'">
      <path
        d="M4 17V19C4 20.1046 4.89543 21 6 21H18C19.1046 21 20 20.1046 20 19V17"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
      />
      <path
        d="M12 3V15M12 15L7 10M12 15L17 10"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </template>

    <!-- Export / Upload -->
    <template v-else-if="name === 'export' || name === 'arrow-up-tray'">
      <path
        d="M4 17V19C4 20.1046 4.89543 21 6 21H18C19.1046 21 20 20.1046 20 19V17"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
      />
      <path
        d="M12 15V3M12 3L7 8M12 3L17 8"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </template>

    <!-- Info / About -->
    <template v-else-if="name === 'info' || name === 'about'">
      <circle
        cx="12"
        cy="12"
        r="9"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
      />
      <path
        d="M12 8V8.01M12 11V16"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
      />
    </template>

    <!-- Rss / Feed -->
    <template v-else-if="name === 'rss' || name === 'feed'">
      <path
        d="M4 11C8.41828 11 12 14.5817 12 19"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
      />
      <path
        d="M4 4C12.2843 4 19 10.7157 19 19"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
      />
      <circle cx="5" cy="19" r="1.5" fill="currentColor" />
    </template>

    <!-- App / General -->
    <template v-else-if="name === 'app' || name === 'sliders'">
      <path
        d="M4 21V14M4 10V3M12 21V12M12 8V3M20 21V16M20 12V3M1 14H7M9 8H15M17 16H23"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
      />
    </template>

    <!-- Keyboard / Shortcuts -->
    <template v-else-if="name === 'keyboard'">
      <rect
        x="2.5"
        y="5.5"
        width="19"
        height="13"
        rx="2.5"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
      />
      <path
        d="M6 9H6.01M10 9H10.01M14 9H14.01M18 9H18.01M6 12H6.01M10 12H10.01M14 12H14.01M18 12H18.01M8 15H16"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </template>

    <!-- Sources -->
    <template v-else-if="name === 'sources' || name === 'antenna'">
      <path
        d="M4.93 4.93C1.02 8.84 1.02 15.16 4.93 19.07M19.07 4.93C22.98 8.84 22.98 15.16 19.07 19.07"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
      />
      <path
        d="M7.76 7.76C5.02 10.49 5.02 14.91 7.76 17.64M16.24 7.76C18.98 10.49 18.98 14.91 16.24 17.64"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
        stroke-linecap="round"
      />
      <circle cx="12" cy="12.7" r="2" :stroke="props.color" :stroke-width="props.strokeWidth" fill="currentColor" />
    </template>

    <!-- Database / Data -->
    <template v-else-if="name === 'data' || name === 'cylinder'">
      <ellipse
        cx="12"
        cy="6"
        rx="8"
        ry="3"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
      />
      <path
        d="M4 6V12C4 13.66 7.58 15 12 15C16.42 15 20 13.66 20 12V6"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
      />
      <path
        d="M4 12V18C4 19.66 7.58 21 12 21C16.42 21 20 19.66 20 18V12"
        :stroke="props.color"
        :stroke-width="props.strokeWidth"
      />
    </template>

    <!-- Fallback / Default -->
    <template v-else>
      <circle cx="12" cy="12" r="8" :stroke="props.color" :stroke-width="props.strokeWidth" />
    </template>
  </svg>
</template>

<style scoped>
.sf-icon {
  display: inline-block;
  vertical-align: middle;
  flex-shrink: 0;
  transition: transform 0.15s cubic-bezier(0.16, 1, 0.3, 1), color 0.15s ease;
}
</style>
