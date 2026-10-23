import { describe, expect, it } from 'vitest'
import {
  DEFAULT_MINI_BACKGROUND_COLOR,
  MINI_AUTO_SCROLL_SPEED_MAX,
  MINI_AUTO_SCROLL_SPEED_MIN,
  exceedsMiniDragThreshold,
  isLightMiniBackground,
  miniBackgroundFill,
  normalizeMiniColor,
  normalizeMiniOpacity,
  resolveMiniKeyAction,
  stepMiniAutoScrollSpeed,
} from './miniReader'

describe('resolveMiniKeyAction', () => {
  it('手动阅读时不接管左右键, 交还常规快捷键(方向与常规阅读一致)', () => {
    expect(resolveMiniKeyAction({ key: 'ArrowLeft' }, false)).toBeNull()
    expect(resolveMiniKeyAction({ key: 'ArrowRight' }, false)).toBeNull()
  })

  it('自动滚动时左右键只调速, 不切章', () => {
    expect(resolveMiniKeyAction({ key: 'ArrowLeft' }, true)).toBe('slowerAutoScroll')
    expect(resolveMiniKeyAction({ key: 'ArrowRight' }, true)).toBe('fasterAutoScroll')
  })

  it('/ 开关自动滚动, Esc 隐藏窗口, 上下键滚动', () => {
    expect(resolveMiniKeyAction({ key: '/' }, false)).toBe('toggleAutoScroll')
    expect(resolveMiniKeyAction({ key: 'Escape' }, true)).toBe('hideWindow')
    expect(resolveMiniKeyAction({ key: 'ArrowUp' }, true)).toBe('scrollUp')
    expect(resolveMiniKeyAction({ key: 'ArrowDown' }, false)).toBe('scrollDown')
  })

  it('带修饰键或其它键不接管, 交还常规快捷键', () => {
    expect(resolveMiniKeyAction({ key: 'ArrowLeft', metaKey: true }, false)).toBeNull()
    expect(resolveMiniKeyAction({ key: '/', shiftKey: true }, false)).toBeNull()
    expect(resolveMiniKeyAction({ key: 'f', ctrlKey: true }, false)).toBeNull()
    expect(resolveMiniKeyAction({ key: ' ' }, false)).toBeNull()
  })
})

describe('stepMiniAutoScrollSpeed', () => {
  it('按 12 一档增减, 并夹在 12–300', () => {
    expect(stepMiniAutoScrollSpeed(24, 1)).toBe(36)
    expect(stepMiniAutoScrollSpeed(24, -1)).toBe(12)
    expect(stepMiniAutoScrollSpeed(12, -1)).toBe(MINI_AUTO_SCROLL_SPEED_MIN)
    expect(stepMiniAutoScrollSpeed(300, 1)).toBe(MINI_AUTO_SCROLL_SPEED_MAX)
  })

  it('常规设置里调出的非整档速度先对齐到最近一档', () => {
    expect(stepMiniAutoScrollSpeed(30, 1)).toBe(48)
    expect(stepMiniAutoScrollSpeed(500, -1)).toBe(MINI_AUTO_SCROLL_SPEED_MAX)
  })
})

describe('迷你外观取值', () => {
  it('非法颜色回落默认值', () => {
    expect(normalizeMiniColor('red', DEFAULT_MINI_BACKGROUND_COLOR)).toBe(DEFAULT_MINI_BACKGROUND_COLOR)
    expect(normalizeMiniColor('#ABC', DEFAULT_MINI_BACKGROUND_COLOR)).toBe('#aabbcc')
  })

  it('透明度夹在 0–1, 脏值回落不透明', () => {
    expect(normalizeMiniOpacity(1.5)).toBe(1)
    expect(normalizeMiniOpacity(-1)).toBe(0)
    expect(normalizeMiniOpacity('0.3')).toBe(1)
    expect(normalizeMiniOpacity(0.164)).toBe(0.16)
  })

  it('背景填充: 不透明时给原色, 否则给 rgba', () => {
    expect(miniBackgroundFill('#282c34', 1)).toBe('#282c34')
    expect(miniBackgroundFill('#282c34', 0.16)).toBe('rgba(40, 44, 52, 0.16)')
    expect(miniBackgroundFill('#0f110f', 0)).toBe('rgba(15, 17, 15, 0)')
  })

  it('按亮度判断深浅底色', () => {
    expect(isLightMiniBackground('#f2efe7')).toBe(true)
    expect(isLightMiniBackground('#282c34')).toBe(false)
  })
})

describe('exceedsMiniDragThreshold', () => {
  it('移动不足 4px 仍算点击', () => {
    expect(exceedsMiniDragThreshold(2, 3)).toBe(false)
    expect(exceedsMiniDragThreshold(4, 1)).toBe(true)
  })
})
