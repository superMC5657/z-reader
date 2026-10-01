<script setup lang="ts">
import { getCurrentWindow } from '@tauri-apps/api/window'

const props = withDefaults(
  defineProps<{
    closable?: boolean
    minimizable?: boolean
    maximizable?: boolean
  }>(),
  {
    closable: true,
    minimizable: true,
    maximizable: true,
  }
)

const emit = defineEmits<{
  close: []
  minimize: []
  maximize: []
}>()

let appWindow: ReturnType<typeof getCurrentWindow> | null = null
try {
  appWindow = getCurrentWindow()
} catch (e) {
  console.warn('TrafficLights: unable to get current window', e)
}

async function onWindowClose() {
  if (!props.closable) return
  try {
    await appWindow?.close()
  } catch (err) {
    console.warn('Failed to close window:', err)
  }
  emit('close')
}

async function onWindowMinimize() {
  if (!props.minimizable) return
  try {
    await appWindow?.minimize()
  } catch (err) {
    console.warn('Failed to minimize window:', err)
  }
  emit('minimize')
}

async function onWindowMaximize() {
  if (!props.maximizable) return
  try {
    await appWindow?.toggleMaximize()
  } catch (err) {
    console.warn('Failed to toggle maximize window:', err)
  }
  emit('maximize')
}
</script>

<template>
  <div class="traffic-lights">
    <button
      class="traffic-light close"
      :class="{ disabled: !closable }"
      :disabled="!closable"
      title="Close (关闭)"
      @click.stop="onWindowClose"
    >
      <svg viewBox="0 0 10 10" class="tl-icon">
        <path d="M2.5 2.5L7.5 7.5M7.5 2.5L2.5 7.5" stroke="#4c0000" stroke-width="1.3" stroke-linecap="round" />
      </svg>
    </button>
    <button
      class="traffic-light minimize"
      :class="{ disabled: !minimizable }"
      :disabled="!minimizable"
      title="Minimize (最小化)"
      @click.stop="onWindowMinimize"
    >
      <svg viewBox="0 0 10 10" class="tl-icon">
        <path d="M2 5H8" stroke="#5c3c00" stroke-width="1.3" stroke-linecap="round" />
      </svg>
    </button>
    <button
      class="traffic-light maximize"
      :class="{ disabled: !maximizable }"
      :disabled="!maximizable"
      title="Zoom / Maximize (最大化/还原)"
      @click.stop="onWindowMaximize"
    >
      <svg viewBox="0 0 10 10" class="tl-icon">
        <path d="M2.5 7.5L7.5 2.5M7.5 2.5H4.5M7.5 2.5V5.5" stroke="#004c00" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
    </button>
  </div>
</template>

<style scoped>
.traffic-lights {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0.15rem 0.2rem;
}

.traffic-light {
  width: 12px;
  height: 12px;
  border-radius: 50%;
  position: relative;
  box-shadow: inset 0 0 0 0.5px rgba(0, 0, 0, 0.18);
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  padding: 0;
  border: none;
  outline: none;
  transition: opacity 0.15s ease, filter 0.15s ease, transform 0.1s ease;
}

.traffic-light:hover:not(:disabled) {
  filter: brightness(0.92);
}

.traffic-light:active:not(:disabled) {
  transform: scale(0.9);
  filter: brightness(0.8);
}

.traffic-light:disabled,
.traffic-light.disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.traffic-light.close {
  background: #ff5f56;
}

.traffic-light.minimize {
  background: #ffbd2e;
}

.traffic-light.maximize {
  background: #27c93f;
}

.tl-icon {
  width: 7.5px;
  height: 7.5px;
  opacity: 0;
  transition: opacity 0.15s ease;
  pointer-events: none;
}

.traffic-lights:hover .tl-icon {
  opacity: 1;
}
</style>
