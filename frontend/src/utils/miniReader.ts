/** 迷你模式的浮窗阅读规则, 移植自 reader_mac(macOS 原生悬浮阅读器)。
 *
 *  迷你模式有自己的一套外观(文字色/背景色/背景透明度), 与常规阅读主题分开存:
 *  小窗悬在别的应用之上, 适合它的配色往往和整窗阅读不同, 互不干扰才能两边都调顺手。
 *  字号、字重、行距仍与常规阅读共用 —— 旧版迷你模式就是这样, 不改变它。 */

import { normalizeReaderColor } from './readerColor'

export interface MiniColorPreset {
  name: string
  color: string
}

/** 与 reader_mac 的 ReaderColorPreset 一致。 */
export const MINI_TEXT_PRESETS: MiniColorPreset[] = [
  { name: '暖白', color: '#f7f6ef' },
  { name: '浅绿', color: '#d8efd9' },
  { name: '浅橙', color: '#ffd6a1' },
  { name: '浅红', color: '#f3b7b1' },
]

export const MINI_BACKGROUND_PRESETS: MiniColorPreset[] = [
  { name: '墨黑', color: '#0f110f' },
  { name: '蓝灰', color: '#20252f' },
  { name: '暖白', color: '#f2efe7' },
  { name: '暗红', color: '#40292c' },
]

/** 默认外观取自用户在 reader_mac 里实际在用的设置(浅绿字、#282c34 底、不透明)。 */
export const DEFAULT_MINI_TEXT_COLOR = '#d8efd9'
export const DEFAULT_MINI_BACKGROUND_COLOR = '#282c34'
export const DEFAULT_MINI_BACKGROUND_OPACITY = 1

/** 自动滚动速度(像素/秒)在迷你模式里按 reader_mac 的档位调: 12 一档, 12–300。 */
export const MINI_AUTO_SCROLL_SPEED_MIN = 12
export const MINI_AUTO_SCROLL_SPEED_MAX = 300
export const MINI_AUTO_SCROLL_SPEED_STEP = 12

/** 滚轮/触控板手势结束多久后恢复自动滚动。常规阅读是 1.2 秒, 迷你模式按
 *  reader_mac 取 0.1 秒: 小窗里往回瞟一眼就该接着滚, 不必停顿。 */
export const MINI_AUTO_SCROLL_RESUME_MS = 100

export function normalizeMiniColor(value: unknown, fallback: string): string {
  return normalizeReaderColor(value) || fallback
}

export function normalizeMiniOpacity(value: unknown): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) return DEFAULT_MINI_BACKGROUND_OPACITY
  return Math.round(Math.min(1, Math.max(0, value)) * 100) / 100
}

/** #rrggbb + 不透明度 → rgba()。透明度只作用于背景, 文字始终不透明。 */
export function miniBackgroundFill(color: string, opacity: number): string {
  const hex = normalizeMiniColor(color, DEFAULT_MINI_BACKGROUND_COLOR)
  const alpha = normalizeMiniOpacity(opacity)
  if (alpha >= 1) return hex
  const r = parseInt(hex.slice(1, 3), 16)
  const g = parseInt(hex.slice(3, 5), 16)
  const b = parseInt(hex.slice(5, 7), 16)
  return `rgba(${r}, ${g}, ${b}, ${alpha})`
}

/** 背景偏亮时用浅色方案(滚动条、表单控件跟着走)。阈值同 reader_mac。 */
export function isLightMiniBackground(color: string): boolean {
  const hex = normalizeMiniColor(color, DEFAULT_MINI_BACKGROUND_COLOR)
  const r = parseInt(hex.slice(1, 3), 16) / 255
  const g = parseInt(hex.slice(3, 5), 16) / 255
  const b = parseInt(hex.slice(5, 7), 16) / 255
  return 0.2126 * r + 0.7152 * g + 0.0722 * b > 0.52
}

export function stepMiniAutoScrollSpeed(speed: number, direction: 1 | -1): number {
  const base = Number.isFinite(speed) ? speed : MINI_AUTO_SCROLL_SPEED_MIN
  const snapped = Math.round(base / MINI_AUTO_SCROLL_SPEED_STEP) * MINI_AUTO_SCROLL_SPEED_STEP
  const next = snapped + direction * MINI_AUTO_SCROLL_SPEED_STEP
  return Math.min(MINI_AUTO_SCROLL_SPEED_MAX, Math.max(MINI_AUTO_SCROLL_SPEED_MIN, next))
}

export type MiniKeyAction =
  | 'toggleAutoScroll'
  | 'hideWindow'
  | 'scrollUp'
  | 'scrollDown'
  | 'nextChapter'
  | 'prevChapter'
  | 'slowerAutoScroll'
  | 'fasterAutoScroll'

/** 迷你模式的固定键位(reader_mac 的 ReaderKeyboardRouter)。
 *
 *  - `/` 开关自动滚动; `Esc` 停止自动滚动并隐藏窗口。
 *  - 上/下: 按住持续滚动正文。
 *  - 左/右: 自动滚动时减速/加速; 否则左键下一章、右键上一章。
 *    左右与常规阅读相反, 这是 reader_mac 的约定(见其 CONTEXT.md「章节导航」)。
 *
 *  带任何修饰键都不接管, 交还给常规快捷键。未列出的键返回 null。 */
export function resolveMiniKeyAction(
  event: { key: string; ctrlKey?: boolean; metaKey?: boolean; altKey?: boolean; shiftKey?: boolean },
  isAutoScrolling: boolean,
): MiniKeyAction | null {
  if (event.ctrlKey || event.metaKey || event.altKey || event.shiftKey) return null
  switch (event.key) {
    case '/':
      return 'toggleAutoScroll'
    case 'Escape':
      return 'hideWindow'
    case 'ArrowUp':
      return 'scrollUp'
    case 'ArrowDown':
      return 'scrollDown'
    case 'ArrowLeft':
      return isAutoScrolling ? 'slowerAutoScroll' : 'nextChapter'
    case 'ArrowRight':
      return isAutoScrolling ? 'fasterAutoScroll' : 'prevChapter'
    default:
      return null
  }
}

/** 按住正文移动超过这个距离(像素)才算拖动窗口, 否则仍是一次点击。 */
export const MINI_DRAG_THRESHOLD = 4

export function exceedsMiniDragThreshold(dx: number, dy: number): boolean {
  return dx * dx + dy * dy > MINI_DRAG_THRESHOLD * MINI_DRAG_THRESHOLD
}
