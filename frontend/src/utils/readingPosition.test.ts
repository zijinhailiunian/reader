import { describe, expect, it } from 'vitest'
import {
  resolveReadingPosition,
  shouldPreferServerReadingPosition,
  toMillis,
} from './readingPosition'

const pos = (chapterIndex: number, progress: number, extra = {}) => ({ chapterIndex, progress, ...extra })

describe('readingPosition', () => {
  describe('同章: 本地逐页快照优先', () => {
    it('keeps local progress when the shelf record is behind', () => {
      const local = pos(47, 0.62, { updatedAt: 2000 })
      const shelf = pos(47, 0.18, { updatedAt: 1000 })

      expect(resolveReadingPosition(local, shelf)).toBe(local)
      expect(shouldPreferServerReadingPosition(local, shelf)).toBe(false)
    })

    // 回归(向后读): 跨章瞬间会写一条 (新章, 0), 且 durChapterTime 会被"打开这一章"
    // 刷新成最新。按时间戳判会让这条 0 赢, 用户重开回到该章第一页。
    it('keeps in-chapter progress when the shelf record says chapter start', () => {
      const local = pos(12, 0.4, { updatedAt: 1792841567429 })
      const shelf = pos(12, 0, { updatedAt: 1792841567 })

      expect(resolveReadingPosition(local, shelf)).toBe(local)
    })

    // 回归(向前读): 用户往回翻重读, 本地进度比书架小。旧实现只看「差值 ≥ 2%」,
    // 于是每次重开都被拉回之前读得更远的位置。
    it('keeps local progress when the user read backwards (shelf is further ahead)', () => {
      const local = pos(25, 0.1, { updatedAt: 1792843000000 })
      const shelf = pos(25, 0.85, { updatedAt: 1792842900 })

      expect(resolveReadingPosition(local, shelf)).toBe(local)
      expect(shouldPreferServerReadingPosition(local, shelf)).toBe(false)
    })

    it('keeps the local paragraph anchor when both agree', () => {
      const local = pos(47, 0.61, { paragraphIndex: 8, updatedAt: 2000 })
      const shelf = pos(47, 0.6, { updatedAt: 3000 })

      expect(resolveReadingPosition(local, shelf)).toBe(local)
    })
  })

  describe('跨章: 采信书架(主动跳章只写在那里)', () => {
    it('uses the shelf when it points at a different chapter', () => {
      const local = pos(12, 0.9, { updatedAt: 1000 })
      const shelf = pos(13, 0, { updatedAt: 2000 })

      expect(resolveReadingPosition(local, shelf)).toBe(shelf)
      expect(shouldPreferServerReadingPosition(local, shelf)).toBe(true)
    })

    // 往回翻越章边界后, 本地已经是新(更小)的章, 书架还停在旧的更大章。
    // 这种情况本地才是对的 —— 但翻页越界会先把新章写进 store, 所以
    // 正常路径下两边章号其实是一致的; 这里锁定"本地缺失才用书架"的兜底。
    it('uses the shelf when there is no local snapshot', () => {
      const shelf = pos(13, 0.25, { updatedAt: 2000 })
      expect(resolveReadingPosition(null, shelf)).toBe(shelf)
      expect(resolveReadingPosition(null, null)).toBe(null)
    })
  })

  describe('远端(手机端/WebDAV)', () => {
    it('adopts remote only when it is the furthest chapter', () => {
      const local = pos(10, 0.5)
      const shelf = pos(10, 0.2)
      const remote = pos(12, 0)

      expect(resolveReadingPosition(local, shelf, remote)).toBe(remote)
    })

    it('ignores remote when it is behind', () => {
      const local = pos(10, 0.5)
      const remote = pos(8, 0.9)

      expect(resolveReadingPosition(local, null, remote)).toBe(local)
    })
  })

  it('normalizes second-level durChapterTime to milliseconds', () => {
    expect(toMillis(1_792_845_667)).toBe(1_792_845_667_000)
    expect(toMillis(1_792_845_667_429)).toBe(1_792_845_667_429)
    expect(toMillis(0)).toBe(0)
    expect(toMillis(undefined)).toBe(0)
    expect(toMillis(null)).toBe(0)
  })
})
