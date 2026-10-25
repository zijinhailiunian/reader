export interface ReadingPositionSnapshot {
  chapterIndex: number
  progress: number
  paragraphIndex?: number
  paragraphProgress?: number
  updatedAt?: number
}

export const READER_POSITION_PREFIX = 'reader-position:'

/** 书架里的 `durChapterTime` 是 Unix **秒**(Rust `now_ts()`), 本地快照是毫秒。
 *  统一折算成毫秒, 免得任何基于时间戳的取舍把书架记录当成 1970 年的数据。
 *  阈值与 `utils/bookshelfSort.ts` 的 toMillis 保持一致 (1e11 ≈ 5138 年)。 */
export function toMillis(value: number | undefined | null): number {
  if (typeof value !== 'number' || !Number.isFinite(value) || value <= 0) return 0
  return value < 1e11 ? value * 1000 : value
}

export function readSavedReadingPosition(
  bookUrl: string | undefined | null,
): ReadingPositionSnapshot | null {
  if (!bookUrl) return null
  try {
    const raw = localStorage.getItem(`${READER_POSITION_PREFIX}${bookUrl}`)
    if (!raw) return null
    const parsed = JSON.parse(raw) as Partial<ReadingPositionSnapshot>
    if (typeof parsed?.chapterIndex !== 'number') return null
    return {
      chapterIndex: parsed.chapterIndex,
      progress: typeof parsed.progress === 'number'
        ? Math.max(0, Math.min(1, parsed.progress))
        : 0,
      paragraphIndex: typeof parsed.paragraphIndex === 'number' ? parsed.paragraphIndex : undefined,
      paragraphProgress: typeof parsed.paragraphProgress === 'number' ? parsed.paragraphProgress : undefined,
      updatedAt: typeof parsed.updatedAt === 'number' ? parsed.updatedAt : 0,
    }
  } catch {
    return null
  }
}

/**
 * 决定「重新打开这本书应该落到的位置」。
 *
 * 候选来源:
 * - `local`: 本机 localStorage 的段落级快照。**每次翻页/滚动都写**(120ms 防抖),
 *   退出时再强制写一次, 带 `paragraphIndex` 段落锚点。它就是「用户最后一眼停在哪」。
 * - `shelf`: 书架记录(`durChapterIndex`/`durChapterPos`)。由 10 秒自动保存、
 *   换章、退出时各写一次, 比本地稀疏; 换章瞬间还会先写一条 `(新章, 0)`。
 *
 * **规则只有一条: 章号相同用本地, 章号不同用书架。**
 *
 * 为什么不能比较「谁读得更远」或「谁时间戳更新」:
 *
 * - 阅读是**双向**的, 用户会往回翻重读。按"更远"判断, 往回翻后的位置会被判成
 *   "更旧", 每次打开都被拉回之前读得更远的地方 —— 这正是「向前阅读后退出重开,
 *   又跳回去」的原因。按"时间戳更新"同样不可靠: `durChapterTime` 只表示"最近一次
 *   进入这一章", 换章瞬间写下的 `(新章, 0)` 时间戳永远是最新的。
 * - 章号不同时**只能是书架对**: 翻页越界(`nextChapter`/`prevChapter`)会立刻把
 *   新章写进 store, 本地快照紧接着就按新章落盘; 而"本地还停在旧章、书架已是新章"
 *   只出现在一种情况 —— 用户从目录/详情页**主动跳章**, 目标章写在书架记录里,
 *   本地快照自然还是上一次读的那章。此时必须采信书架。
 *
 * `remote`(手机端/WebDAV)只在它比两边都靠后时采用: 那是用户换了设备继续读。
 */
export function resolveReadingPosition(
  local: ReadingPositionSnapshot | null,
  shelf: ReadingPositionSnapshot | null,
  remote: ReadingPositionSnapshot | null = null,
): ReadingPositionSnapshot | null {
  if (remote && remote.chapterIndex > Math.max(local?.chapterIndex ?? -1, shelf?.chapterIndex ?? -1)) {
    return remote
  }
  if (local && shelf) {
    return local.chapterIndex === shelf.chapterIndex ? local : shelf
  }
  return local ?? shelf ?? null
}

/**
 * 该不该用「书架/会话快照」的位置覆盖本地精确位置。
 *
 * 与 `resolveReadingPosition` 同一套判据: 同章永远保留本地(段落级精度更高),
 * 只有跨章(主动跳章)或本地快照缺失时才采信书架。
 */
export function shouldPreferServerReadingPosition(
  local: ReadingPositionSnapshot | null,
  server: ReadingPositionSnapshot | null,
): boolean {
  if (!server) return false
  if (!local) return true
  return local.chapterIndex !== server.chapterIndex
}
