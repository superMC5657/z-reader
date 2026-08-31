import { check, type Update } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'
import { ref } from 'vue'

export type UpdateStatus =
  | 'idle'
  | 'checking'
  | 'available'
  | 'downloading'
  | 'downloaded'
  | 'up-to-date'
  | 'error'

export interface UpdateState {
  status: UpdateStatus
  currentVersion: string
  newVersion?: string
  releaseNotes?: string
  progress: number
  downloadedBytes: number
  totalBytes: number
  error?: string
}

export const updateState = ref<UpdateState>({
  status: 'idle',
  currentVersion: '',
  progress: 0,
  downloadedBytes: 0,
  totalBytes: 0,
})

let pendingUpdate: Update | null = null

export async function checkForUpdates() {
  if (updateState.value.status === 'checking' || updateState.value.status === 'downloading') {
    return
  }

  updateState.value.status = 'checking'
  updateState.value.error = undefined

  try {
    const update = await check()
    if (update) {
      pendingUpdate = update
      updateState.value = {
        status: 'available',
        currentVersion: update.currentVersion,
        newVersion: update.version,
        releaseNotes: update.body || '',
        progress: 0,
        downloadedBytes: 0,
        totalBytes: 0,
      }
    } else {
      pendingUpdate = null
      updateState.value.status = 'up-to-date'
    }
  } catch (err) {
    pendingUpdate = null
    const msg = err instanceof Error ? err.message : String(err)
    updateState.value.status = 'error'
    updateState.value.error = msg
    console.error('Check for updates failed:', err)
  }
}

export async function startDownloadAndInstall() {
  if (!pendingUpdate || updateState.value.status === 'downloading') return

  updateState.value.status = 'downloading'
  updateState.value.progress = 0
  updateState.value.downloadedBytes = 0
  updateState.value.totalBytes = 0
  updateState.value.error = undefined

  try {
    let total = 0
    let downloaded = 0
    await pendingUpdate.downloadAndInstall((event) => {
      if (event.event === 'Started') {
        total = event.data.contentLength ?? 0
        downloaded = 0
        updateState.value.totalBytes = total
      } else if (event.event === 'Progress') {
        downloaded += event.data.chunkLength
        updateState.value.downloadedBytes = downloaded
        if (total > 0) {
          updateState.value.progress = Math.min(100, Math.round((downloaded / total) * 100))
        }
      } else if (event.event === 'Finished') {
        updateState.value.progress = 100
      }
    })

    updateState.value.status = 'downloaded'
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err)
    updateState.value.status = 'error'
    updateState.value.error = msg
    console.error('Update download/install failed:', err)
  }
}

export async function restartApp() {
  try {
    await relaunch()
  } catch (err) {
    console.error('Relaunch failed:', err)
  }
}
