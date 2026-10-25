<template>
  <div
    class="rule-manager"
    :style="{
      background: theme.popup,
      color: theme.fontColor,
      '--rm-font': theme.fontColor,
      '--rm-popup': theme.popup,
    }"
  >
    <header class="panel-head">
      <div class="panel-head-main">
        <h3>内容净化</h3>
        <p class="panel-sub">{{ subtitle }}</p>
      </div>
      <div class="panel-head-actions">
        <button class="btn" type="button" :disabled="importing" @click="openImportPanel">导入</button>
        <button class="btn btn-primary" type="button" @click="openEditModal()">新增</button>
        <button class="icon-btn" type="button" aria-label="返回" title="返回" @click="handleClose">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="m15 18-6-6 6-6" /></svg>
        </button>
      </div>
    </header>

    <div class="panel-body">
      <div v-if="loading" class="state">正在读取规则…</div>

      <div v-else-if="!store.replaceRules.length" class="empty">
        <p class="empty-title">还没有自定义规则</p>
        <p class="empty-text">
          规则按「排序」从小到大依次执行，用来清掉正文里的广告尾巴、站点水印这类内容。
        </p>
        <div class="empty-actions">
          <button class="btn btn-primary" type="button" @click="openEditModal()">新增规则</button>
          <button class="btn" type="button" @click="openImportPanel">导入规则</button>
        </div>
      </div>

      <template v-else>
        <section class="group">
          <div class="group-head">
            <h4>规则列表</h4>
            <span class="group-note">按排序执行</span>
          </div>
          <div class="card card-rows">
            <article
              v-for="rule in sortedRules"
              :key="rule.id"
              class="rule-row"
              :class="{ disabled: !rule.isEnabled }"
            >
              <div class="rule-main">
                <div class="rule-title">
                  <span class="rule-name">{{ rule.name }}</span>
                  <span v-if="rule.isRegex" class="tag">正则</span>
                  <!-- Legado 规则是 Java 正则, 少数写法在 JS 里没有等价物。标出来,
                       免得用户以为「导入了却没效果」是阅读器的问题。 -->
                  <span
                    v-if="ruleIssues.get(rule.name)"
                    class="tag tag-warn"
                    :title="ruleIssues.get(rule.name)"
                  >无法执行</span>
                </div>
                <p class="rule-scope">{{ describeScope(rule.scope) }}</p>
                <dl class="rule-patterns">
                  <div class="pattern-line">
                    <dt>匹配</dt>
                    <dd :title="rule.pattern">{{ rule.pattern }}</dd>
                  </div>
                  <div class="pattern-line">
                    <dt>替换</dt>
                    <dd :title="rule.replacement || '(空)'">{{ rule.replacement || '(空)' }}</dd>
                  </div>
                </dl>
              </div>
              <div class="rule-ops">
                <button
                  class="switch"
                  type="button"
                  role="switch"
                  :aria-checked="rule.isEnabled"
                  :aria-label="`启用规则 ${rule.name}`"
                  :class="{ on: rule.isEnabled }"
                  @click="toggleRule(rule)"
                ></button>
                <div class="rule-buttons">
                  <button class="link-btn" type="button" @click="openEditModal(rule)">编辑</button>
                  <button class="link-btn danger" type="button" @click="handleDelete(rule)">删除</button>
                </div>
              </div>
            </article>
          </div>
        </section>

        <p class="hint">
          对正文的改动只影响阅读显示，不会改写原书文件；关闭某条规则后重新打开本章即可看到原文。
        </p>
      </template>
    </div>

    <!-- 编辑面板：盖在本面板之上，左上角返回。 -->
    <Transition name="fade">
      <div v-if="editingRule" class="sub-panel" :style="{ background: theme.popup }">
        <header class="sub-head">
          <button class="icon-btn" type="button" aria-label="返回" @click="editingRule = null">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="m15 18-6-6 6-6" /></svg>
          </button>
          <h4>{{ editingRule.id ? '编辑规则' : '新增规则' }}</h4>
        </header>

        <div class="sub-body">
          <div class="field">
            <label for="rule-name">名称</label>
            <input id="rule-name" v-model="editingRule.name" placeholder="例如：去除站点广告尾巴">
          </div>

          <div class="field">
            <label for="rule-pattern">匹配内容</label>
            <textarea
              id="rule-pattern"
              v-model="editingRule.pattern"
              rows="3"
              placeholder="要匹配的文本或正则表达式"
            ></textarea>
          </div>

          <div class="field">
            <label for="rule-replacement">替换为</label>
            <input id="rule-replacement" v-model="editingRule.replacement" placeholder="留空表示直接删除">
          </div>

          <div class="field">
            <label>生效范围</label>
            <div class="segmented">
              <button
                type="button"
                :class="{ on: getScopeMode(editingRule) === 'global' }"
                @click="setScopeMode('global')"
              >全部书籍</button>
              <button
                type="button"
                :class="{ on: getScopeMode(editingRule) === 'book' }"
                @click="setScopeMode('book')"
              >当前书籍</button>
              <button
                type="button"
                :class="{ on: getScopeMode(editingRule) === 'source' }"
                @click="setScopeMode('source')"
              >当前书源</button>
            </div>
            <p class="field-hint">{{ describeScope(editingRule.scope) }}</p>
          </div>

          <div class="field field-inline">
            <label class="check">
              <input v-model="editingRule.isRegex" type="checkbox">
              <span>按正则表达式匹配</span>
            </label>
            <label class="order">
              <span>排序</span>
              <input v-model.number="editingRule.order" type="number" min="1" step="1">
            </label>
          </div>
        </div>

        <footer class="sub-foot">
          <button class="btn" type="button" @click="editingRule = null">取消</button>
          <button class="btn btn-primary" type="button" @click="handleSave">保存</button>
        </footer>
      </div>
    </Transition>

    <!-- 导入面板：复用同一个子面板层级，入口只有直链与本地文件。 -->
    <Transition name="fade">
      <div v-if="importPanelOpen" class="sub-panel" :style="{ background: theme.popup }">
        <header class="sub-head">
          <button class="icon-btn" type="button" aria-label="返回" :disabled="importing" @click="closeImportPanel">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="m15 18-6-6 6-6" /></svg>
          </button>
          <h4>导入规则</h4>
        </header>

        <div class="sub-body">
          <div class="field">
            <label for="rule-import-url">规则 JSON 链接</label>
            <input
              id="rule-import-url"
              v-model="importUrl"
              type="url"
              placeholder="https://example.test/replace-rules.json"
              :disabled="importing"
              @keyup.enter="importFromUrl"
            >
          </div>

          <p class="field-hint">
            支持 Legado 净化规则的 JSON 数组。以 <code>@js:</code> 开头的脚本替换会在
            Rust 沙盒里执行；需要脚本生成匹配式的规则没有等价能力，导入时会跳过。
          </p>

          <button class="btn" type="button" :disabled="importing" @click="importFromUrl">
            {{ importing ? '导入中…' : '从网络导入' }}
          </button>

          <div class="divider"><span>或</span></div>

          <button class="btn" type="button" :disabled="importing" @click="localFileInputRef?.click()">
            选择本地 JSON 文件
          </button>
          <input
            ref="localFileInputRef"
            type="file"
            accept=".json,application/json"
            hidden
            @change="handleLocalFile"
          >

          <p class="field-hint">导入按规则名合并，同名规则会被覆盖。</p>
        </div>
      </div>
    </Transition>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useReaderStore } from '../../stores/reader'
import { useAppStore } from '../../stores/app'
import { saveReplaceRule, saveReplaceRules, deleteReplaceRule } from '../../api/replaceRule'
import {
  buildReplaceRuleImportMessage,
  normalizeLegadoReplaceRules,
} from '../../utils/replaceRuleImport'
import { compileReplaceRule } from '../../utils/replaceRuleRegex'
import type { ReplaceRule } from '../../types'

const store = useReaderStore()
const appStore = useAppStore()

/**
 * 净化规则面板。只在阅读页的面板栈里用（`store.openPanel('rule')`），关闭即
 * `store.backPanel()`；没有「独立模式」—— 主设置抽屉里曾经复用它，那个入口已
 * 移除，因为规则的作用域（当前书籍 / 当前书源）本身就要有阅读上下文。
 */
function handleClose() {
  store.backPanel()
}

const theme = computed(() => store.chromeTheme)
const loading = ref(false)
const editingRule = ref<ReplaceRule | null>(null)

const subtitle = computed(() => (
  store.replaceRules.length
    ? `${store.replaceRules.length} 条规则 · 按排序依次执行`
    : '清理正文里的广告与杂项'
))

/* ─── 导入面板 ─── */
const importPanelOpen = ref(false)
const importUrl = ref('')
const importing = ref(false)
const localFileInputRef = ref<HTMLInputElement | null>(null)

const sortedRules = computed(() => {
  return [...store.replaceRules].sort((a, b) => a.order - b.order)
})

/**
 * 规则名 → 无法执行的原因。
 *
 * 两个来源缺一不可：
 *  - 静态编译：规则本身写错（正则不支持、脚本语法错），一进列表就该标出来；
 *  - 运行时失败：`store.failedReplaceRules` 记录本次阅读里真正执行失败的规则
 *    （超时、脚本抛错、内存超限）。只做静态检查的话，「点了没反应」无从排查。
 */
const ruleIssues = computed(() => {
  const issues = new Map<string, string>(Object.entries(store.failedReplaceRules))
  for (const rule of store.replaceRules) {
    const compiled = compileReplaceRule(rule.pattern, rule.isRegex, rule.replacement)
    if (compiled.kind === 'error') issues.set(rule.name, compiled.error)
  }
  return issues
})

/** 只返回范围本身，前缀「范围:」由模板里的位置表达，避免每行都重复两个字。 */
function describeScope(scope?: string) {
  if (!scope || scope === '*') return '全部书籍'
  if (scope.startsWith('source:')) {
    return `当前书源 (${scope.slice('source:'.length)})`
  }
  if (scope.startsWith('book:')) {
    return `当前书籍 (${scope.slice('book:'.length)})`
  }
  return scope
}

function getScopeMode(rule: ReplaceRule) {
  const scope = rule.scope || '*'
  if (scope.startsWith('source:')) return 'source'
  if (scope.startsWith('book:')) return 'book'
  return 'global'
}

onMounted(async () => {
  loading.value = true
  await store.fetchReplaceRules()
  loading.value = false
})

function openEditModal(rule?: ReplaceRule) {
  importPanelOpen.value = false
  if (rule) {
    editingRule.value = { ...rule }
  } else {
    editingRule.value = {
      id: 0,
      name: '',
      pattern: '',
      replacement: '',
      scope: '*',
      isEnabled: true,
      isRegex: false,
      order: store.replaceRules.length + 1
    }
  }
}

function setScopeMode(mode: 'global' | 'book' | 'source') {
  if (!editingRule.value) return
  if (mode === 'global') {
    editingRule.value.scope = '*'
    return
  }
  if (!store.book) return
  editingRule.value.scope = mode === 'source'
    ? `source:${store.book.origin}`
    : `book:${store.book.bookUrl}`
}

async function handleSave() {
  if (!editingRule.value) return
  if (!editingRule.value.name || !editingRule.value.pattern) {
    appStore.showToast('请填写名称和匹配内容', 'warning')
    return
  }
  try {
    await saveReplaceRule(editingRule.value)
    await store.fetchReplaceRules()
    editingRule.value = null
  } catch (e: any) {
    appStore.showToast(e.message || '保存失败', 'error')
  }
}

/** 开关按钮先翻转本地值再保存（原实现靠 checkbox 的 v-model 先改值）。 */
async function toggleRule(rule: ReplaceRule) {
  rule.isEnabled = !rule.isEnabled
  try {
    await saveReplaceRule(rule)
    await store.fetchReplaceRules()
  } catch (e: any) {
    appStore.showToast(e.message || '规则状态保存失败', 'error')
    // saveReplaceRule 可能已在请求发出前就改了本地对象; 用服务端数据恢复真实状态,
    // 不能简单取反(旧实现失败时反而把用户刚点的开关翻回去)。
    const restored = await store.fetchReplaceRules()
    // 恢复也失败时本地开关已不可信, 必须明确告知 —— 否则界面停在一个
    // 既非"用户点的值"也非"服务端值"的状态上而无人察觉。
    if (!restored) appStore.showToast('规则状态同步失败，请重新打开本面板确认', 'warning')
  }
}

async function handleDelete(rule: ReplaceRule) {
  const ok = await appStore.confirmDialog(`确定删除规则 "${rule.name}" 吗？`, { title: '删除规则', danger: true })
  if (!ok) return
  try {
    await deleteReplaceRule(rule)
    await store.fetchReplaceRules()
  } catch (e: any) {
    appStore.showToast(e.message || '删除失败', 'error')
  }
}

/* ─── 导入 ─── */

function openImportPanel() {
  // 两个面板都绝对定位铺满本面板, 同时开会互相盖住, 打开一个就关掉另一个。
  editingRule.value = null
  importUrl.value = ''
  importPanelOpen.value = true
}

function closeImportPanel() {
  if (importing.value) return
  importPanelOpen.value = false
}

/** 直链导入用前端 fetch，而不是后端 readRemoteSourceFile。
 *
 *  为什么不能走后端: 那条命令会把响应反序列化成 BookSource，净化规则的
 *  pattern/replacement 字段名对不上，会被整段丢掉，拿回来只剩空规则。
 *  为什么 WebView 里直连可行: 该站返回 `access-control-allow-origin: *`，
 *  且 tauri.conf.json 的 csp 是 null，不拦 fetch。
 *  跨域被拒时 fetch 只抛 `TypeError: Failed to fetch`，对用户毫无信息量，
 *  所以这里统一换成可读提示。 */
async function fetchRulesJson(url: string): Promise<unknown> {
  let response: Response
  try {
    response = await fetch(url, { cache: 'no-store' })
  } catch {
    throw new Error('无法从该地址获取规则：请检查网络连接，或该站点不允许跨域访问')
  }
  if (!response.ok) {
    throw new Error(`获取规则失败：服务器返回 ${response.status}`)
  }
  const text = await response.text()
  try {
    return JSON.parse(text)
  } catch {
    throw new Error('该地址返回的不是合法 JSON（可能被重定向到了网页）')
  }
}

/** 归一化 + 保存 + 提示，直链与本地文件两个入口共用。
 *  全部被跳过时只提示、不写入 —— 保存空数组会让人误以为导入成功了。 */
async function applyImportedRules(raw: unknown) {
  const result = normalizeLegadoReplaceRules(raw)
  const feedback = buildReplaceRuleImportMessage(result)
  if (!result.rules.length) {
    appStore.showToast(feedback.text, 'error')
    return
  }
  await saveReplaceRules(result.rules)
  const refreshed = await store.fetchReplaceRules()
  importPanelOpen.value = false
  // 保存成功但列表没拉回来时不能说"导入成功": 面板上看到的仍是旧列表,
  // 用户会以为规则没进去。明确提示需要重开面板确认。
  if (!refreshed) {
    appStore.showToast(`${feedback.text}，但规则列表刷新失败，请重新打开本面板确认`, 'warning')
    return
  }
  appStore.showToast(feedback.text, feedback.type)
}

async function importFromUrl() {
  if (importing.value) return
  const url = importUrl.value.trim()
  if (!url) {
    appStore.showToast('请输入规则 JSON 链接', 'warning')
    return
  }
  importing.value = true
  try {
    await applyImportedRules(await fetchRulesJson(url))
  } catch (error) {
    appStore.showToast((error as Error).message || '导入失败', 'error')
  } finally {
    importing.value = false
  }
}

async function handleLocalFile(event: Event) {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  if (!file) return
  importing.value = true
  try {
    let raw: unknown
    try {
      raw = JSON.parse(await file.text())
    } catch {
      throw new Error('文件内容不是合法 JSON')
    }
    await applyImportedRules(raw)
  } catch (error) {
    appStore.showToast((error as Error).message || '导入失败', 'error')
  } finally {
    importing.value = false
    // 清空 value: 否则再次选同一个文件不会触发 change
    input.value = ''
  }
}
</script>

<style scoped>
/* 与阅读设置 / 离线缓存同一套观感：颜色从 --rm-font（阅读主题文字色）派生，
 * 暗色阅读主题下不会出现写死 rgba(0,0,0,x) 造成的整块不可见。 */
.rule-manager {
  --rm-line: color-mix(in srgb, var(--rm-font, currentColor) 9%, transparent);
  --rm-line-strong: color-mix(in srgb, var(--rm-font, currentColor) 18%, transparent);
  --rm-label: color-mix(in srgb, var(--rm-font, currentColor) 70%, transparent);
  --rm-muted: color-mix(in srgb, var(--rm-font, currentColor) 46%, transparent);
  --rm-card: color-mix(in srgb, var(--rm-font, currentColor) 3.2%, transparent);
  --rm-card-border: color-mix(in srgb, var(--rm-font, currentColor) 7%, transparent);
  --rm-field: color-mix(in srgb, var(--rm-font, currentColor) 5%, transparent);
  --rm-accent: var(--color-primary, #c97f3a);
  --rm-accent-fill: color-mix(in srgb, var(--rm-accent) 13%, transparent);
  --rm-accent-line: color-mix(in srgb, var(--rm-accent) 46%, transparent);
  --rm-danger: #c43f3a;

  display: flex;
  flex-direction: column;
  height: 100%;
  position: relative;
}

/* ─── 顶栏 ─── */
.panel-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  padding: 22px 20px 14px;
  flex-shrink: 0;
}

.panel-head-main {
  min-width: 0;
}

.panel-head h3 {
  margin: 0;
  font-size: 16px;
  font-weight: 600;
}

.panel-sub {
  margin: 4px 0 0;
  font-size: 12px;
  color: var(--rm-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.panel-head-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 0 0 auto;
}

.panel-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 0 20px 24px;
  display: flex;
  flex-direction: column;
  gap: 20px;
  -webkit-overflow-scrolling: touch;
  overscroll-behavior: contain;
}

/* ─── 分组与卡片 ─── */
.group {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.group-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 10px;
  padding: 0 3px;
}

.group-head h4 {
  margin: 0;
  font-size: 11.5px;
  font-weight: 600;
  letter-spacing: 0.1em;
  color: var(--rm-muted);
}

.group-note {
  font-size: 11px;
  color: var(--rm-muted);
}

.card {
  border-radius: 14px;
  border: 1px solid var(--rm-card-border);
  background: var(--rm-card);
}

.card-rows {
  display: flex;
  flex-direction: column;
  padding: 2px 14px;
}

/* ─── 规则行 ─── */
.rule-row {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  padding: 12px 0;
  transition: opacity 0.18s;
}

.rule-row + .rule-row {
  border-top: 1px solid var(--rm-line);
}

.rule-row.disabled {
  opacity: 0.5;
}

.rule-main {
  min-width: 0;
  flex: 1;
}

.rule-title {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
}

.rule-name {
  font-size: 13.5px;
  font-weight: 600;
}

.tag {
  padding: 1px 6px;
  border-radius: 5px;
  font-size: 10.5px;
  color: var(--rm-muted);
  background: color-mix(in srgb, var(--rm-font, currentColor) 8%, transparent);
}

/* 无法执行: 用告警色而不是主题色, 避免和「正则」标签混在一起看不见。 */
.tag.tag-warn {
  color: #b4551f;
  background: color-mix(in srgb, #d4972d 16%, transparent);
  cursor: help;
}

.rule-scope {
  margin: 4px 0 0;
  font-size: 11.5px;
  color: var(--rm-muted);
}

.rule-patterns {
  margin: 6px 0 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.pattern-line {
  display: flex;
  gap: 6px;
  min-width: 0;
}

.pattern-line dt {
  flex: 0 0 auto;
  font-size: 11px;
  color: var(--rm-muted);
}

.pattern-line dd {
  margin: 0;
  min-width: 0;
  font-size: 11.5px;
  font-family: 'Cascadia Mono', 'Consolas', monospace;
  color: var(--rm-label);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* ─── 行内操作 ─── */
.rule-ops {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 8px;
  flex: 0 0 auto;
}

.rule-buttons {
  display: flex;
  gap: 2px;
}

/* 开关：与设置面板同款，纯 CSS，无输入控件残留边框。 */
.switch {
  position: relative;
  width: 38px;
  height: 22px;
  padding: 0;
  border: 0;
  border-radius: 999px;
  background: color-mix(in srgb, var(--rm-font, currentColor) 18%, transparent);
  cursor: pointer;
  transition: background 0.18s;
}

.switch::after {
  content: "";
  position: absolute;
  top: 2px;
  left: 2px;
  width: 18px;
  height: 18px;
  border-radius: 50%;
  background: #fff;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.32);
  transition: transform 0.18s;
}

.switch.on {
  background: var(--rm-accent);
}

.switch.on::after {
  transform: translateX(16px);
}

.switch:hover {
  background: color-mix(in srgb, var(--rm-font, currentColor) 26%, transparent);
}

.switch.on:hover {
  background: color-mix(in srgb, var(--rm-accent) 86%, var(--rm-font, currentColor));
}

/* ─── 按钮 ─── */
.btn {
  min-height: 30px;
  padding: 5px 12px;
  border-radius: 8px;
  border: 1px solid var(--rm-line-strong);
  background: transparent;
  color: var(--rm-label);
  font-size: 12.5px;
  cursor: pointer;
  transition: background 0.15s, color 0.15s, border-color 0.15s;
}

.btn:hover:not(:disabled) {
  background: color-mix(in srgb, var(--rm-font, currentColor) 6%, transparent);
  color: var(--rm-font, currentColor);
}

.btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.btn-primary {
  background: var(--rm-accent-fill);
  border-color: var(--rm-accent-line);
  color: var(--rm-font, currentColor);
  font-weight: 600;
}

.btn-primary:hover:not(:disabled) {
  background: color-mix(in srgb, var(--rm-accent) 19%, transparent);
}

.link-btn {
  padding: 2px 6px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--rm-accent);
  font-size: 11.5px;
  cursor: pointer;
}

.link-btn:hover {
  background: var(--rm-accent-fill);
}

.link-btn.danger {
  color: var(--rm-danger);
}

.link-btn.danger:hover {
  background: color-mix(in srgb, var(--rm-danger) 12%, transparent);
}

.icon-btn {
  flex: 0 0 auto;
  width: 28px;
  height: 28px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 8px;
  border: 1px solid transparent;
  background: transparent;
  color: var(--rm-label);
  cursor: pointer;
  transition: background 0.15s, color 0.15s;
}

.icon-btn svg {
  width: 16px;
  height: 16px;
}

.icon-btn:hover:not(:disabled) {
  background: color-mix(in srgb, var(--rm-font, currentColor) 6%, transparent);
  color: var(--rm-font, currentColor);
}

/* ─── 空态 / 加载 ─── */
.state,
.empty {
  padding: 40px 8px;
  text-align: center;
}

.empty-title {
  margin: 0;
  font-size: 13.5px;
  font-weight: 600;
  color: var(--rm-label);
}

.empty-text {
  margin: 8px auto 0;
  max-width: 34ch;
  font-size: 12px;
  line-height: 1.7;
  color: var(--rm-muted);
}

.empty-actions {
  margin-top: 16px;
  display: flex;
  gap: 8px;
  justify-content: center;
  flex-wrap: wrap;
}

.state {
  font-size: 12.5px;
  color: var(--rm-muted);
}

.hint {
  margin: 0;
  padding: 0 3px;
  font-size: 11.5px;
  line-height: 1.6;
  color: var(--rm-muted);
}

/* ─── 子面板（编辑 / 导入） ─── */
.sub-panel {
  position: absolute;
  inset: 0;
  z-index: 10;
  display: flex;
  flex-direction: column;
}

.sub-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 22px 20px 14px;
  flex-shrink: 0;
}

.sub-head h4 {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
}

.sub-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 0 20px 24px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.sub-foot {
  flex-shrink: 0;
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 12px 20px 20px;
  border-top: 1px solid var(--rm-line);
}

.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.field > label {
  font-size: 12px;
  color: var(--rm-label);
}

.field input[type="text"],
.field input[type="url"],
.field input:not([type]),
.field textarea,
.order input {
  width: 100%;
  box-sizing: border-box;
  padding: 8px 11px;
  border-radius: 9px;
  border: 1px solid var(--rm-line-strong);
  background: var(--rm-field);
  color: inherit;
  font: inherit;
  font-size: 13px;
  outline: none;
  transition: border-color 0.15s, box-shadow 0.15s, background 0.15s;
}

.field textarea {
  resize: vertical;
  min-height: 72px;
  line-height: 1.6;
}

.field input:focus,
.field textarea:focus,
.order input:focus {
  border-color: var(--rm-accent);
  box-shadow: 0 0 0 3px var(--rm-accent-fill);
  background: color-mix(in srgb, var(--rm-font, currentColor) 8%, transparent);
}

.field input::placeholder,
.field textarea::placeholder {
  color: var(--rm-muted);
}

.field-hint {
  margin: 0;
  font-size: 11.5px;
  line-height: 1.6;
  color: var(--rm-muted);
}

.field-hint code {
  padding: 1px 4px;
  border-radius: 4px;
  font-size: 11px;
  background: color-mix(in srgb, var(--rm-font, currentColor) 9%, transparent);
}

.field-inline {
  flex-direction: row;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  flex-wrap: wrap;
}

.check {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  font-size: 12.5px;
  color: var(--rm-label);
  cursor: pointer;
}

.check input {
  accent-color: var(--rm-accent);
}

.order {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12.5px;
  color: var(--rm-label);
}

.order input {
  width: 64px;
  font-variant-numeric: tabular-nums;
}

.segmented {
  display: inline-flex;
  gap: 2px;
  padding: 2px;
  border-radius: 9px;
  background: var(--rm-field);
  max-width: 100%;
  flex-wrap: wrap;
}

.segmented button {
  min-height: 28px;
  padding: 0 12px;
  border: 0;
  border-radius: 7px;
  background: transparent;
  color: var(--rm-label);
  font-size: 12.5px;
  cursor: pointer;
  transition: background 0.15s, color 0.15s, box-shadow 0.15s;
}

.segmented button:hover:not(.on) {
  color: var(--rm-font, currentColor);
  background: color-mix(in srgb, var(--rm-font, currentColor) 7%, transparent);
}

.segmented button.on {
  background: color-mix(in srgb, #fff 26%, var(--rm-popup));
  color: var(--rm-font, currentColor);
  font-weight: 600;
  box-shadow: 0 1px 3px rgba(20, 25, 30, 0.18);
}

.divider {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 11px;
  color: var(--rm-muted);
}

.divider::before,
.divider::after {
  content: "";
  flex: 1;
  height: 1px;
  background: var(--rm-line);
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

@media (prefers-reduced-motion: reduce) {
  .rule-manager * {
    transition-duration: 0.01ms !important;
  }
}

@media (max-width: 420px) {
  .panel-head {
    flex-wrap: wrap;
  }

  .panel-head-actions {
    width: 100%;
    justify-content: flex-end;
  }

  .rule-row {
    flex-direction: column;
    gap: 10px;
  }

  .rule-ops {
    flex-direction: row;
    align-items: center;
    justify-content: space-between;
    width: 100%;
  }
}
</style>
