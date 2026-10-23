<template>
  <div
    ref="panelRef"
    class="mini-reader-panel"
    role="menu"
    aria-label="迷你模式设置"
    :style="{ top: `${position.top}px`, left: `${position.left}px` }"
    @click.stop
    @mousedown.stop
    @contextmenu.prevent.stop
    @wheel.stop
  >
    <button type="button" class="mrp-row" role="menuitem" @click="emit('bookshelf')">
      <svg class="mrp-icon" viewBox="0 0 24 24"><path d="M6 22V4a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2v18l-6-4z" /></svg>
      <span class="mrp-title">书架</span>
    </button>
    <button type="button" class="mrp-row" role="menuitem" @click="emit('chapters')">
      <svg class="mrp-icon" viewBox="0 0 24 24"><path d="M21 12h-8M21 6H8M21 18h-8M3 6v4c0 1.1.9 2 2 2h3M3 10v6c0 1.1.9 2 2 2h3" /></svg>
      <span class="mrp-title">章节</span>
      <svg class="mrp-icon" viewBox="0 0 24 24"><path d="m9 18 6-6-6-6" /></svg>
    </button>

    <div class="mrp-separator"></div>

    <div class="mrp-control">
      <div class="mrp-label"><span>字号</span><span>{{ config.fontSize }} px</span></div>
      <div class="mrp-stepper">
        <button type="button" aria-label="减小字号" :disabled="config.fontSize <= READER_FONT_SIZE_MIN" @click="stepFontSize(-1)">−</button>
        <span>{{ config.fontSize }}</span>
        <button type="button" aria-label="增大字号" :disabled="config.fontSize >= READER_FONT_SIZE_MAX" @click="stepFontSize(1)">+</button>
      </div>
    </div>
    <div class="mrp-control">
      <div class="mrp-label"><span>行距</span><span>{{ config.lineHeight.toFixed(1) }}x</span></div>
      <div class="mrp-stepper">
        <button type="button" aria-label="减小行距" :disabled="config.lineHeight <= LINE_HEIGHT_MIN" @click="stepLineHeight(-1)">−</button>
        <span>{{ config.lineHeight.toFixed(1) }}</span>
        <button type="button" aria-label="增大行距" :disabled="config.lineHeight >= LINE_HEIGHT_MAX" @click="stepLineHeight(1)">+</button>
      </div>
    </div>
    <div class="mrp-control">
      <div class="mrp-label"><span>文字颜色</span><span>文字始终不透明</span></div>
      <div class="mrp-colors">
        <button
          v-for="preset in MINI_TEXT_PRESETS"
          :key="preset.color"
          type="button"
          class="mrp-swatch"
          :class="{ selected: config.miniTextColor === preset.color }"
          :style="{ background: preset.color }"
          :title="`${preset.name}文字`"
          :aria-label="`${preset.name}文字`"
          @click="store.updateConfig('miniTextColor', preset.color)"
        ></button>
        <button
          v-if="appStore.isMacDesktop"
          type="button"
          class="mrp-picker"
          title="从屏幕吸取文字颜色"
          aria-label="从屏幕吸取文字颜色"
          :disabled="sampling"
          @click="sampleScreenColor('miniTextColor')"
        >
          <svg viewBox="0 0 24 24"><path d="m2 22 1-1h3l9-9M3 21v-3l9-9M15 6l3.4-3.4a2.1 2.1 0 1 1 3 3L18 9l.4.4a2.1 2.1 0 1 1-3 3l-3.8-3.8a2.1 2.1 0 1 1 3-3l.4.4z" /></svg>
        </button>
        <label v-else class="mrp-picker" title="自定义文字颜色">
          <svg viewBox="0 0 24 24"><path d="m2 22 1-1h3l9-9M3 21v-3l9-9M15 6l3.4-3.4a2.1 2.1 0 1 1 3 3L18 9l.4.4a2.1 2.1 0 1 1-3 3l-3.8-3.8a2.1 2.1 0 1 1 3-3l.4.4z" /></svg>
          <input
            type="color"
            aria-label="自定义文字颜色"
            :value="config.miniTextColor"
            @input="store.updateConfig('miniTextColor', normalizeMiniColor(($event.target as HTMLInputElement).value, config.miniTextColor))"
          >
        </label>
      </div>
    </div>
    <div class="mrp-control">
      <div class="mrp-label"><span>背景颜色</span><span>{{ appStore.supportsTransparentWindow ? '透明度独立' : '' }}</span></div>
      <div class="mrp-colors">
        <button
          v-for="preset in MINI_BACKGROUND_PRESETS"
          :key="preset.color"
          type="button"
          class="mrp-swatch"
          :class="{ selected: config.miniBackgroundColor === preset.color }"
          :style="{ background: preset.color }"
          :title="`${preset.name}背景`"
          :aria-label="`${preset.name}背景`"
          @click="store.updateConfig('miniBackgroundColor', preset.color)"
        ></button>
        <button
          v-if="appStore.isMacDesktop"
          type="button"
          class="mrp-picker"
          title="从屏幕吸取背景颜色"
          aria-label="从屏幕吸取背景颜色"
          :disabled="sampling"
          @click="sampleScreenColor('miniBackgroundColor')"
        >
          <svg viewBox="0 0 24 24"><path d="m2 22 1-1h3l9-9M3 21v-3l9-9M15 6l3.4-3.4a2.1 2.1 0 1 1 3 3L18 9l.4.4a2.1 2.1 0 1 1-3 3l-3.8-3.8a2.1 2.1 0 1 1 3-3l.4.4z" /></svg>
        </button>
        <label v-else class="mrp-picker" title="自定义背景颜色">
          <svg viewBox="0 0 24 24"><path d="m2 22 1-1h3l9-9M3 21v-3l9-9M15 6l3.4-3.4a2.1 2.1 0 1 1 3 3L18 9l.4.4a2.1 2.1 0 1 1-3 3l-3.8-3.8a2.1 2.1 0 1 1 3-3l.4.4z" /></svg>
          <input
            type="color"
            aria-label="自定义背景颜色"
            :value="config.miniBackgroundColor"
            @input="store.updateConfig('miniBackgroundColor', normalizeMiniColor(($event.target as HTMLInputElement).value, config.miniBackgroundColor))"
          >
        </label>
      </div>
    </div>
    <div v-if="appStore.supportsTransparentWindow" class="mrp-control">
      <div class="mrp-label"><span>背景透明度</span><span>不影响文字</span></div>
      <div class="mrp-opacity">
        <input
          type="range"
          min="0"
          max="100"
          step="1"
          aria-label="背景透明度"
          :value="opacityPercent"
          :style="{ '--fill': `${opacityPercent}%` }"
          @input="store.updateConfig('miniBackgroundOpacity', normalizeMiniOpacity(Number(($event.target as HTMLInputElement).value) / 100))"
        >
        <span>{{ opacityPercent }}%</span>
      </div>
    </div>

    <div class="mrp-separator"></div>

    <button type="button" class="mrp-row" role="menuitem" @click="emit('toggleAutoScroll')">
      <svg class="mrp-icon" viewBox="0 0 24 24"><path d="M12 5v14M19 12l-7 7-7-7" /></svg>
      <span class="mrp-title">{{ store.isAutoScrolling ? '停止自动滚动' : '自动滚动' }}</span>
      <span class="mrp-shortcut">/</span>
    </button>
    <button type="button" class="mrp-row" role="menuitem" @click="emit('exitMini')">
      <svg class="mrp-icon" viewBox="0 0 24 24"><path d="M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7" /></svg>
      <span class="mrp-title">退出迷你模式</span>
    </button>
    <button type="button" class="mrp-row" role="menuitem" @click="emit('hide')">
      <svg class="mrp-icon" viewBox="0 0 24 24"><path d="M10.7 5.1A10.7 10.7 0 0 1 22 12a13.5 13.5 0 0 1-1.7 2.7M14.1 14.2a3 3 0 0 1-4.2-4.2M17.5 17.5A10.8 10.8 0 0 1 2 12a13.2 13.2 0 0 1 4.4-5.4M2 2l20 20" /></svg>
      <span class="mrp-title">隐藏窗口</span>
      <span class="mrp-shortcut">Esc</span>
    </button>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, ref } from 'vue'
import { useReaderStore } from '../../stores/reader'
import { useAppStore } from '../../stores/app'
import { invokeRaw } from '../../api/invoke'
import { READER_FONT_SIZE_MAX, READER_FONT_SIZE_MIN } from '../../utils/readerFontSize'
import {
  MINI_BACKGROUND_PRESETS,
  MINI_TEXT_PRESETS,
  normalizeMiniColor,
  normalizeMiniOpacity,
} from '../../utils/miniReader'

/** 迷你模式的右键面板, 对应 reader_mac 的 ReaderControlPanelView。
 *  面板放不下时贴着窗口边缘摆, 小窗(240px 高)里超出部分在面板内滚动。 */
const props = defineProps<{ x: number; y: number }>()
const emit = defineEmits<{
  bookshelf: []
  chapters: []
  toggleAutoScroll: []
  exitMini: []
  hide: []
}>()

/** 与常规阅读设置的行距范围一致(ReadSettings 的 stepFloat)。 */
const LINE_HEIGHT_MIN = 1.0
const LINE_HEIGHT_MAX = 3.0
const EDGE = 6

const store = useReaderStore()
const appStore = useAppStore()
const config = computed(() => store.config)
const opacityPercent = computed(() => Math.round(config.value.miniBackgroundOpacity * 100))

const panelRef = ref<HTMLElement>()
const position = ref({ top: props.y, left: props.x })

/** macOS: 系统屏幕吸管(同 reader_mac 的 NSColorSampler)。按 Esc 取消时不改颜色。 */
const sampling = ref(false)

async function sampleScreenColor(key: 'miniTextColor' | 'miniBackgroundColor') {
  if (sampling.value) return
  sampling.value = true
  try {
    const color = await invokeRaw<string | null>('sample_screen_color')
    if (color) store.updateConfig(key, normalizeMiniColor(color, config.value[key]))
  } catch (error) {
    appStore.showToast((error as Error).message || '取色失败', 'error')
  } finally {
    sampling.value = false
  }
}

function stepFontSize(delta: number) {
  const next = Math.min(READER_FONT_SIZE_MAX, Math.max(READER_FONT_SIZE_MIN, config.value.fontSize + delta))
  store.updateConfig('fontSize', next)
}

function stepLineHeight(direction: 1 | -1) {
  const next = Math.round((config.value.lineHeight + direction * 0.1) * 10) / 10
  store.updateConfig('lineHeight', Math.min(LINE_HEIGHT_MAX, Math.max(LINE_HEIGHT_MIN, next)))
}

onMounted(async () => {
  await nextTick()
  const panel = panelRef.value
  if (!panel) return
  const width = panel.offsetWidth
  const height = panel.offsetHeight
  position.value = {
    left: Math.max(EDGE, Math.min(props.x, window.innerWidth - width - EDGE)),
    top: Math.max(EDGE, Math.min(props.y, window.innerHeight - height - EDGE)),
  }
})
</script>

<style scoped>
/* 配色与尺寸照搬 reader_mac(ReaderBlueprintColor): 浅色底 + 绿色强调,
   不跟随阅读主题 —— 面板要在任意正文配色上都清楚可读。 */
.mini-reader-panel {
  --mrp-accent: #1e7a50;
  --mrp-text: #252824;
  --mrp-label: #5f635d;
  --mrp-shortcut: #7d817a;
  --mrp-border: #aeb2aa;
  --mrp-separator: #d7d9d3;
  --mrp-control-border: #c4c7c0;
  --mrp-stepper: #ecece8;
  --mrp-stepper-hover: #dde8e1;
  --mrp-swatch-border: #b9bcb5;
  --mrp-swatch-selected: #2f5f45;
  position: fixed;
  z-index: 3000;
  width: 224px;
  max-height: calc(100vh - 12px);
  overflow-y: auto;
  overscroll-behavior: contain;
  scrollbar-width: none;
  padding: 6px;
  box-sizing: border-box;
  border: 1px solid var(--mrp-border);
  border-radius: 6px;
  background: #fafaf8;
  color: var(--mrp-text);
  color-scheme: light;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.22);
  font-family: -apple-system, BlinkMacSystemFont, 'PingFang SC', 'Microsoft YaHei', sans-serif;
  font-size: 12px;
  line-height: 1.3;
  user-select: none;
  -webkit-user-select: none;
}

.mini-reader-panel::-webkit-scrollbar {
  display: none;
}

.mrp-row {
  display: flex;
  align-items: center;
  gap: 7px;
  width: 100%;
  height: 29px;
  padding: 0 7px;
  border: 0;
  border-radius: 4px;
  background: transparent;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: default;
}

.mrp-row:hover,
.mrp-row:focus-visible {
  outline: none;
  background: var(--mrp-accent);
  color: #fff;
}

.mrp-icon {
  flex: 0 0 auto;
  width: 15px;
  height: 15px;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.7;
  stroke-linecap: round;
  stroke-linejoin: round;
}

.mrp-title {
  flex: 1;
  min-width: 0;
  white-space: nowrap;
}

.mrp-shortcut {
  font-size: 10px;
  color: var(--mrp-shortcut);
}

.mrp-row:hover .mrp-shortcut {
  color: rgba(255, 255, 255, 0.75);
}

.mrp-separator {
  height: 1px;
  margin: 5px 4px;
  background: var(--mrp-separator);
}

.mrp-control {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 5px 8px;
}

.mrp-label {
  display: flex;
  justify-content: space-between;
  gap: 6px;
  font-size: 10px;
  color: var(--mrp-label);
}

.mrp-stepper {
  display: flex;
  height: 26px;
  border: 1px solid var(--mrp-control-border);
  border-radius: 4px;
  overflow: hidden;
}

.mrp-stepper button {
  width: 28px;
  border: 0;
  background: var(--mrp-stepper);
  color: var(--mrp-text);
  font-size: 15px;
  line-height: 1;
  cursor: default;
}

.mrp-stepper button:hover:not(:disabled) {
  background: var(--mrp-stepper-hover);
}

.mrp-stepper button:disabled {
  opacity: 0.35;
}

.mrp-stepper span {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  border-left: 1px solid var(--mrp-control-border);
  border-right: 1px solid var(--mrp-control-border);
  background: #fff;
  font-size: 11px;
}

.mrp-colors {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 24px;
}

.mrp-swatch {
  width: 20px;
  height: 20px;
  padding: 0;
  border: 1px solid var(--mrp-swatch-border);
  border-radius: 50%;
  cursor: default;
}

.mrp-swatch.selected {
  border-color: var(--mrp-swatch-selected);
  box-shadow: 0 0 0 1px #fff inset, 0 0 0 1px var(--mrp-swatch-selected);
}

.mrp-picker {
  position: relative;
  margin-left: auto;
  width: 24px;
  height: 24px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1px solid var(--mrp-control-border);
  border-radius: 4px;
  background: #fff;
  color: var(--mrp-label);
}

.mrp-picker:hover:not(:disabled) {
  border-color: var(--mrp-accent);
  background: var(--mrp-accent);
  color: #fff;
}

.mrp-picker svg {
  width: 13px;
  height: 13px;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.7;
  stroke-linecap: round;
  stroke-linejoin: round;
}

button.mrp-picker {
  padding: 0;
  cursor: default;
}

/* 原生取色控件铺满按钮但不可见: 点按钮即打开系统取色器(macOS 里带吸管)。 */
.mrp-picker input {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  opacity: 0;
  cursor: default;
}

.mrp-opacity {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 16px;
}

.mrp-opacity span {
  width: 34px;
  text-align: right;
  font-size: 10px;
  color: var(--mrp-label);
}

.mrp-opacity input {
  flex: 1;
  min-width: 0;
  height: 8px;
  margin: 0;
  border: 1px solid #b2b2b2;
  border-radius: 4px;
  background: linear-gradient(to right, var(--mrp-accent) var(--fill), #efefef var(--fill));
  -webkit-appearance: none;
  appearance: none;
}

.mrp-opacity input::-webkit-slider-thumb {
  width: 16px;
  height: 16px;
  border: 0;
  border-radius: 50%;
  background: var(--mrp-accent);
  -webkit-appearance: none;
  appearance: none;
}
</style>
