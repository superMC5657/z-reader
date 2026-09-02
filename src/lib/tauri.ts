import { invoke } from '@tauri-apps/api/core'
import type {
  AppStats,
  GetItemsParams,
  Group,
  Item,
  Rule,
  RuleBackfillResult,
  RuleInput,
  Settings,
  Source,
} from '../types'

export const getSources = () => invoke<Source[]>('get_sources')
export const getGroups = () => invoke<Group[]>('get_groups')
export const createGroup = (name: string) => invoke<Group>('create_group', { name })
export const renameGroup = (id: number, name: string) => invoke<void>('rename_group', { id, name })
export const deleteGroup = (id: number) => invoke<void>('delete_group', { id })
export const setGroupExpanded = (id: number, expanded: boolean) =>
  invoke<void>('set_group_expanded', { id, expanded })

export const addSource = (url: string, groupId: number | null) =>
  invoke<Source>('add_source', { url, groupId })
export const removeSource = (id: number) => invoke<void>('remove_source', { id })
export const renameSource = (id: number, title: string) => invoke<void>('rename_source', { id, title })
export const setSourceGroup = (id: number, groupId: number | null) =>
  invoke<void>('set_source_group', { id, groupId })
export const fetchSources = (ids?: number[]) => invoke<number>('fetch_sources', { ids: ids ?? null })

export const getItems = (params: GetItemsParams) => invoke<Item[]>('get_items', { params })
export const getItem = (id: number) => invoke<Item>('get_item', { id })
export const markRead = (ids: number[], read: boolean) => invoke<void>('mark_read', { ids, read })
export const markAllRead = (scope?: string, scopeId?: number | null) =>
  invoke<void>('mark_all_read', { scope: scope ?? null, scopeId: scopeId ?? null })
export const star = (id: number, starred: boolean) => invoke<void>('star', { id, starred })
export const setItemHidden = (id: number, hidden: boolean) => invoke<void>('set_item_hidden', { id, hidden })
export const fetchFullContent = (id: number) => invoke<void>('fetch_full_content', { id })

export const getSettings = () => invoke<Settings>('get_settings')
export const saveSettings = (settings: Settings) => invoke<void>('save_settings', { settings })

export interface OpmlImportResult {
  groupsAdded: number
  sourcesAdded: number
  sourcesExisting: number
}
export const importOpml = (text: string) => invoke<OpmlImportResult>('import_opml', { text })
export const exportOpml = () => invoke<string>('export_opml')
export const refreshFavicon = (id: number) => invoke<string | null>('refresh_favicon', { id })
export const setCustomFavicon = (id: number, dataBase64: string) => invoke<string>('set_custom_favicon', { id, dataBase64 })

// ---------- Phase 2: proxy ----------
export const testProxy = (settings: Settings) => invoke<number>('test_proxy', { settings })

// ---------- Phase 2: regex rules ----------
export const getRules = () => invoke<Rule[]>('get_rules')
export const createRule = (input: RuleInput) => invoke<Rule>('create_rule', { input })
export const updateRule = (id: number, input: RuleInput) => invoke<void>('update_rule', { id, input })
export const deleteRule = (id: number) => invoke<void>('delete_rule', { id })
export const applyRulesBackfill = () => invoke<RuleBackfillResult>('apply_rules_backfill')

// ---------- Phase 2: backup & restore ----------
export const exportBackup = () => invoke<string | null>('export_backup')
export const importBackup = () => invoke<string | null>('import_backup')

// ---------- Phase 2: stats & storage ----------
export const getStats = () => invoke<AppStats>('get_stats')
export const vacuumNow = () => invoke<void>('vacuum_now')
export const cleanupNow = () => invoke<number>('cleanup_now')
