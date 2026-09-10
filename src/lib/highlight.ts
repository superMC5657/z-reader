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
 * Decode HTML entities such as &apos;, &quot;, &amp;, &#39;, &hellip; etc.
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
 * HTML-escape the text, then wrap case-insensitive occurrences of each query
 * token in <mark>. Safe for v-html: the input is escaped first and the only
 * tags ever added are our own <mark> wrappers.
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
