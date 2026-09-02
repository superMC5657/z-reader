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
 * HTML-escape the text, then wrap case-insensitive occurrences of each query
 * token in <mark>. Safe for v-html: the input is escaped first and the only
 * tags ever added are our own <mark> wrappers.
 */
export function highlightText(text: string | null | undefined, query: string): string {
  const escaped = escapeHtml(text ?? '')
  const tokens = query
    .split(/\s+/)
    .filter(Boolean)
    .map((tok) => escapeRegExp(escapeHtml(tok)))
  if (!tokens.length) return escaped
  const re = new RegExp(`(${tokens.join('|')})`, 'gi')
  return escaped.replace(re, '<mark>$1</mark>')
}
