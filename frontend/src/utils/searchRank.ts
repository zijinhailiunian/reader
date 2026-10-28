import type { SearchBook } from '../types'

/**
 * Legado-style relevance ranking for multi-source search results.
 *
 * Buckets (highest priority first):
 *   0. exact title OR author match to the search key
 *   1. search key appears in the kind/tag
 *   2. search key is a substring of title or author
 * Within a bucket, books found in more sources rank higher.
 */
export function rankSearchResults(list: SearchBook[], searchKey: string): SearchBook[] {
  const key = searchKey.trim()
  if (!key) return list

  const tier = (book: SearchBook): number => {
    const name = book.name.trim()
    const author = book.author.trim()
    if (name === key || author === key) return 0
    if (book.kind?.includes(key)) return 1
    if (name.includes(key) || author.includes(key)) return 2
    return 3
  }

  return list.filter((book) => isSearchResultRelevant(book, key)).sort((a, b) => {
    const ta = tier(a)
    const tb = tier(b)
    if (ta !== tb) return ta - tb
    const na = a.bookSourceUrls?.length ?? 1
    const nb = b.bookSourceUrls?.length ?? 1
    return nb - na
  })
}

export function isSearchResultRelevant(
  book: Pick<SearchBook, 'name' | 'author' | 'kind'>,
  searchKey: string,
): boolean {
  const key = normalizeSearchText(searchKey).replace(/^作者[:：]?/, '')
  if (!key) return true

  return [book.name, book.author.replace(/^\s*作者\s*[:：]?\s*/, ''), book.kind || '']
    .some((value) => normalizeSearchText(value).includes(key))
}

/** Keep the complete source hit when merging rows, so the detail modal does
 * not have to rediscover a source that the initial search already found. */
export function initializeSearchResult(book: SearchBook): SearchBook {
  const candidates = mergeSourceCandidates([book], book.sourceCandidates ?? [])
  return {
    ...book,
    bookSourceUrls: Array.from(new Set([
      ...(book.bookSourceUrls ?? []),
      book.origin,
      ...candidates.map((candidate) => candidate.origin),
    ].filter(Boolean))),
    sourceCandidates: candidates,
  }
}

export function mergeSearchResult(existing: SearchBook, incoming: SearchBook): SearchBook {
  const candidates = mergeSourceCandidates(
    [existing, ...(existing.sourceCandidates ?? [])],
    [incoming, ...(incoming.sourceCandidates ?? [])],
  )
  const urls = new Set([
    ...(existing.bookSourceUrls ?? []),
    ...(incoming.bookSourceUrls ?? []),
    existing.origin,
    incoming.origin,
    ...candidates.map((candidate) => candidate.origin),
  ].filter(Boolean))

  return {
    ...existing,
    bookSourceUrls: Array.from(urls),
    sourceCandidates: candidates,
    coverUrl: existing.coverUrl || incoming.coverUrl,
    intro: existing.intro || incoming.intro,
    kind: existing.kind || incoming.kind,
    lastChapter: existing.lastChapter || incoming.lastChapter,
    updateTime: existing.updateTime || incoming.updateTime,
    wordCount: existing.wordCount || incoming.wordCount,
  }
}

/** A source can return several editions or aggregate sites for the same book. */
export function sourceCandidateKey(candidate: Pick<SearchBook, 'origin' | 'bookUrl'>): string {
  return JSON.stringify([candidate.origin.trim().replace(/\/+$/, ''), candidate.bookUrl])
}

/** Keep every distinct link, updating repeated hits with their latest metadata. */
export function mergeSourceCandidates(
  current: readonly SearchBook[],
  incoming: readonly SearchBook[] = [],
): SearchBook[] {
  const candidates = new Map<string, SearchBook>()
  for (const book of [...current, ...incoming]) {
    if (!book.origin.trim() || !book.bookUrl.trim()) continue
    const candidate = searchSourceCandidate(book)
    const key = sourceCandidateKey(candidate)
    const previous = candidates.get(key)
    candidates.set(key, previous ? {
      ...previous,
      ...candidate,
      name: candidate.name || previous.name,
      author: candidate.author || previous.author,
      originName: candidate.originName || previous.originName,
      originGroup: candidate.originGroup || previous.originGroup,
      coverUrl: candidate.coverUrl || previous.coverUrl,
      intro: candidate.intro || previous.intro,
      kind: candidate.kind || previous.kind,
      lastChapter: candidate.lastChapter || previous.lastChapter,
      updateTime: candidate.updateTime || previous.updateTime,
      wordCount: candidate.wordCount || previous.wordCount,
    } : candidate)
  }
  return Array.from(candidates.values())
}

function searchSourceCandidate(book: SearchBook): SearchBook {
  const { bookSourceUrls: _urls, sourceCandidates: _candidates, ...candidate } = book
  return candidate
}

/** Normalized merge key for deduping search results across sources. */
export function searchMergeKey(book: Pick<SearchBook, 'name' | 'author'>): string {
  return `${normalizeSearchName(book.name)}|${normalizeSearchAuthor(book.author)}`
}

/** Strip whitespace so "遮 天" and "遮天" merge. */
function normalizeSearchName(value: string): string {
  return value.trim().toLowerCase().replace(/\s+/g, '')
}

/** Strip whitespace and the "作者：" label prefix so "作者：另一作者" and "另一作者" merge. */
function normalizeSearchAuthor(value: string): string {
  const compact = value.trim().toLowerCase().replace(/\s+/g, '')
  return compact
    .replace(/^作者[:：]/, '')
    .replace(/^作者/, '')
    .replace(/^[:：]/, '')
}

/** Normalize source-switch authors with the same punctuation rules as comparison badges. */
export function normalizeSourceSwitchAuthor(value?: string): string {
  return normalizeSearchAuthor((value || '').replace(/\s+/g, ''))
}

/** Legado's default source switch matches the complete, whitespace-normalized title. */
export function matchesSourceSwitchTitle(currentTitle: string, candidateTitle: string): boolean {
  const current = currentTitle.replace(/\s+/g, '')
  return current.length > 0 && current === candidateTitle.replace(/\s+/g, '')
}

// 标点也要去掉: 书名「穿越神雕：我为西狂」, 用户常输「穿越神雕 我为西狂」, 只去空格会把所有结果都滤掉。
function normalizeSearchText(value: string): string {
  return value.trim().toLocaleLowerCase().replace(/[\s\p{P}\p{S}]+/gu, '')
}
