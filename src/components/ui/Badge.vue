<script setup lang="ts">
interface BadgeProps {
  variant?: 'tint' | 'accent' | 'success' | 'track'
  pill?: boolean
  label?: string | number
  value?: string | number
}

withDefaults(defineProps<BadgeProps>(), {
  variant: 'track',
  pill: true,
  label: undefined,
  value: undefined,
})
</script>

<template>
  <span
    class="badge"
    :class="[
      `badge--${variant}`,
      { 'badge--pill': pill },
    ]"
  >
    <slot>{{ label ?? value ?? '' }}</slot>
  </span>
</template>

<style scoped>
.badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  font-size: 0.74rem;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
  line-height: 1.35;
  letter-spacing: -0.01em;
  padding: 0.08rem 0.5rem;
  white-space: nowrap;
  flex-shrink: 0;
  border-radius: 4px;
  box-sizing: border-box;
  transition: background-color 0.15s ease, color 0.15s ease;
}

.badge--pill {
  border-radius: var(--radius-pill);
}

.badge--track {
  background: var(--bg-track);
  color: var(--text-secondary);
}

.badge--tint {
  background: var(--accent-tint);
  color: var(--accent);
}

.badge--accent {
  background: var(--accent);
  color: #ffffff;
}

.badge--success {
  background: var(--success-tint, rgba(52, 199, 89, 0.12));
  color: var(--success, #34c759);
}
</style>
