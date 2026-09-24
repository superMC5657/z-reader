function escapeHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;')
}

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

/**
 * 解码 HTML 实体字符，如 &apos;、&quot;、&amp;、&#39;、&hellip; 等。
 */
export function decodeHtmlEntities(text: string | null | undefined): string {
  if (!text) return ''
  if (typeof document !== 'undefined') {
    const textarea = document.createElement('textarea')
    textarea.innerHTML = text
    return textarea.value.replace(/&apos;/g, "'").replace(/&#39;/g, "'")
  }
  return text
    .replace(/&apos;/g, "'")
    .replace(/&#39;/g, "'")
    .replace(/&quot;/g, '"')
    .replace(/&amp;/g, '&')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&nbsp;/g, ' ')
}

/**
 * 对文本进行 HTML 转义，然后将查询词中的各分词在文本中（忽略大小写）包裹在 <mark> 标签中。
 * 可安全用于 v-html：输入已预先转义，额外插入的仅有受控的 <mark> 标签。
 */
export function highlightText(text: string | null | undefined, query: string): string {
  const decoded = decodeHtmlEntities(text)
  const escaped = escapeHtml(decoded)
  const tokens = query
    .split(/\s+/)
    .filter(Boolean)
    .map((tok) => escapeRegExp(escapeHtml(tok)))
  if (!tokens.length) return escaped
  const re = new RegExp(`(${tokens.join('|')})`, 'gi')
  return escaped.replace(re, '<mark>$1</mark>')
}
