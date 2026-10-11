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
  SyncStatus,
} from '../types'

/**
 * 薄 invoke 适配层：本模块不拥有任何业务语义，只是把具名 Tauri 命令
 * 收拢到唯一的 `call` 出口之后的一行透传。调用方签名一律不变，
 * 队列真相/回滚/校验等不变式分别由 data store 与 settings store 拥有。
 * 测试用 tests/helpers/invokeHub.ts 替换底层 `invoke` 即完成接缝。
 */
function call<T>(cmd: string, args?: Parameters<typeof invoke>[1]): Promise<T> {
  return invoke<T>(cmd, args)
}

export const getSources = () => call<Source[]>('get_sources')
export const getGroups = () => call<Group[]>('get_groups')
export const createGroup = (name: string) => call<Group>('create_group', { name })
export const renameGroup = (id: number, name: string) => call<void>('rename_group', { id, name })
export const deleteGroup = (id: number) => call<void>('delete_group', { id })
export const setGroupExpanded = (id: number, expanded: boolean) =>
  call<void>('set_group_expanded', { id, expanded })

export const addSource = (url: string, groupId: number | null) =>
  call<Source>('add_source', { url, groupId })
export const removeSource = (id: number) => call<void>('remove_source', { id })
export const renameSource = (id: number, title: string) => call<void>('rename_source', { id, title })
export const setSourceGroup = (id: number, groupId: number | null) =>
  call<void>('set_source_group', { id, groupId })
export const fetchSources = (ids?: number[]) => call<number>('fetch_sources', { ids: ids ?? null })

export const getItems = (params: GetItemsParams) => call<Item[]>('get_items', { params })
export const getItem = (id: number) => call<Item>('get_item', { id })
export const markRead = (ids: number[], read: boolean) => call<void>('mark_read', { ids, read })
export const markAllRead = (scope?: string, scopeId?: number | null) =>
  call<void>('mark_all_read', { scope: scope ?? null, scopeId: scopeId ?? null })
export const star = (id: number, starred: boolean) => call<void>('star', { id, starred })
export const setItemHidden = (id: number, hidden: boolean) => call<void>('set_item_hidden', { id, hidden })
export const fetchFullContent = (id: number) => call<void>('fetch_full_content', { id })

export const getSettings = () => call<Settings>('get_settings')
export const saveSettings = (settings: Settings) => call<void>('save_settings', { settings })

export interface OpmlImportResult {
  groupsAdded: number
  sourcesAdded: number
  sourcesExisting: number
}
export const importOpml = (text: string) => call<OpmlImportResult>('import_opml', { text })
export const exportOpml = () => call<string>('export_opml')
export const refreshFavicon = (id: number) => call<string | null>('refresh_favicon', { id })
export const setCustomFavicon = (id: number, dataBase64: string) => call<string>('set_custom_favicon', { id, dataBase64 })

// ---------- 代理设置 ----------
export const testProxy = (settings: Settings, target?: string | null) =>
  call<number>('test_proxy', { settings, target: target ?? null })

// ---------- 正则自动化规则 ----------
export const getRules = () => call<Rule[]>('get_rules')
export const createRule = (input: RuleInput) => call<Rule>('create_rule', { input })
export const updateRule = (id: number, input: RuleInput) => call<void>('update_rule', { id, input })
export const deleteRule = (id: number) => call<void>('delete_rule', { id })
export const applyRulesBackfill = () => call<RuleBackfillResult>('apply_rules_backfill')

// ---------- 备份与恢复 ----------
export const exportBackup = () => call<string | null>('export_backup')
export const importBackup = () => call<string | null>('import_backup')

// ---------- 存储生命周期与统计 ----------
export const getStats = () => call<AppStats>('get_stats')
export const cleanupNow = () => call<number>('cleanup_now')

// ---------- 云同步（Google Reader API） ----------
export const syncLogin = (serverUrl: string, username: string, password: string) =>
  call<number>('sync_login', { serverUrl, username, password })
export const syncLogout = () => call<void>('sync_logout')
export const syncStatus = () => call<SyncStatus>('sync_status')
export const syncNow = () => call<{ newItems: number; pushed: number; failures: number; subscriptions: number }>('sync_now')
