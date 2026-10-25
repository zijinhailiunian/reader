use scraper::{ElementRef, Html, Selector};
use std::collections::HashSet;
use std::sync::OnceLock;

use crate::parser::rule_analyzer::split_top_level;

#[derive(Clone, Debug, PartialEq)]
enum SelectorBase {
    Css(String),
    Children,
    Text(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IndexMode {
    Select,
    Exclude,
}

#[derive(Clone, Debug, PartialEq)]
enum IndexItem {
    Single(i32),
    Range {
        start: Option<i32>,
        end: Option<i32>,
        step: i32,
    },
}

#[derive(Clone, Debug, PartialEq)]
struct ParsedSelector {
    base: SelectorBase,
    explicit_index: bool,
    index_mode: IndexMode,
    index_items: Vec<IndexItem>,
}

pub fn parse_document(html: &str) -> Html {
    // 片段兜底: HTML5 解析器会丢弃「没有表格/列表上下文」的孤立标签 ——
    // `<tr><td>…` / `<td>…` / `<li>…` 直接交给 `parse_document` 后这些元素
    // 全都不在树里(内部内容会被提到 body 下), 元素级 `select('td a')` /
    // `select('tr')` 因此恒为空。书源脚本把元素 innerHtml 交给宿主选择器引擎
    // 时正是这种形态(元素列表规则交回表格行), 这里按首个标签补上祖先上下文。
    match wrap_bare_fragment(html) {
        Some(wrapped) => Html::parse_document(&wrapped),
        None => Html::parse_document(html),
    }
}

/// `parse_document` 为裸片段补上下文时打在祖先上的标记属性。
const FRAGMENT_WRAP_ATTR: &str = "data-reader-fragment-wrap";
/// 补上去的祖先容器数量(具体标签见 [`wrap_bare_fragment`])。
const FRAGMENT_WRAP_DEPTH: usize = 3;

/// `parse_document` 解析出的 body 的**原始顶层元素**。
///
/// 普通文档就是 `body > *`; 裸片段补过上下文时, 要跳过补上的 `table`/`tbody`/
/// `tr` 这类祖先, 回到片段自己的那一层 —— 否则 `bookList` 交出一串 `<tr>` 时
/// 只会看到一个 `table`。
pub fn document_top_elements(doc: &Html) -> Vec<ElementRef<'_>> {
    static BODY: OnceLock<Option<Selector>> = OnceLock::new();
    let Some(selector) = BODY.get_or_init(|| Selector::parse("body").ok()).as_ref() else {
        return Vec::new();
    };
    let Some(body) = doc.select(selector).next() else {
        return Vec::new();
    };
    let mut current: Vec<ElementRef<'_>> = body.children().filter_map(ElementRef::wrap).collect();
    for _ in 0..FRAGMENT_WRAP_DEPTH {
        if !current
            .iter()
            .any(|el| el.value().attr(FRAGMENT_WRAP_ATTR).is_some())
        {
            break;
        }
        let mut next = Vec::new();
        for el in current {
            if el.value().attr(FRAGMENT_WRAP_ATTR).is_some() {
                next.extend(el.children().filter_map(ElementRef::wrap));
            } else {
                next.push(el);
            }
        }
        current = next;
    }
    current
}

/// 给裸片段补上 HTML5 要求的祖先上下文; 不需要补(或认不出首个标签)时返回 None。
///
/// 只处理「解析器会丢弃」的那几类标签, 其余片段保持原样由调用方处理 ——
/// 过度包装会改变选择器语义(`div > p` 之类)。
fn wrap_bare_fragment(fragment: &str) -> Option<String> {
    let tag = first_tag_name(fragment)?;
    // 完整文档不补(`<html>` / `<body>` 开头时首标签也不在这里)。
    let (prefix, suffix): (&str, &str) = match tag.as_str() {
        "tr" => (
            "<table data-reader-fragment-wrap=\"tr\"><tbody>",
            "</tbody></table>",
        ),
        "td" | "th" => (
            "<table data-reader-fragment-wrap=\"cell\"><tbody><tr>",
            "</tr></tbody></table>",
        ),
        "tbody" | "thead" | "tfoot" | "caption" | "colgroup" => {
            ("<table data-reader-fragment-wrap=\"section\">", "</table>")
        }
        "col" => (
            "<table data-reader-fragment-wrap=\"col\"><colgroup>",
            "</colgroup></table>",
        ),
        "li" => ("<ul data-reader-fragment-wrap=\"li\">", "</ul>"),
        "dt" | "dd" => ("<dl data-reader-fragment-wrap=\"dl\">", "</dl>"),
        "option" | "optgroup" => (
            "<select data-reader-fragment-wrap=\"option\">",
            "</select>",
        ),
        "legend" => (
            "<fieldset data-reader-fragment-wrap=\"legend\">",
            "</fieldset>",
        ),
        _ => return None,
    };
    Some(format!("{prefix}{fragment}{suffix}"))
}

/// 片段里第一个开始标签的名字(小写); 跳过注释/doctype/空白。
fn first_tag_name(fragment: &str) -> Option<String> {
    let mut rest = fragment.trim_start();
    loop {
        if rest.is_empty() {
            return None;
        }
        if let Some(tail) = rest.strip_prefix("<!--") {
            let end = tail.find("-->")?;
            rest = tail[end + 3..].trim_start();
            continue;
        }
        if rest.starts_with("<!") || rest.starts_with("<?") {
            let end = rest.find('>')?;
            rest = rest[end + 1..].trim_start();
            continue;
        }
        let tail = rest.strip_prefix('<')?;
        let name: String = tail
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == ':')
            .collect();
        if name.is_empty() {
            return None;
        }
        return Some(name.to_ascii_lowercase());
    }
}

/// Convert Legado selector format to CSS selector
/// Legado formats:
/// - class.xxx yyy zzz → .xxx.yyy.zzz (multiple classes on one element)
/// - class.xxx or .xxx → .xxx
/// - tag.xxx → xxx (tag name)
/// - id.xxx or #xxx → #xxx
/// - tag.xxx@tag.yyy → nested selectors (split by @)
fn legado_to_css(selector: &str) -> String {
    let selector = selector.trim();

    // Handle special "class." prefix - multiple classes separated by space
    if selector.starts_with("class.") {
        let rest = &selector[6..];
        let classes: Vec<&str> = rest.split_whitespace().collect();
        if classes.len() > 1 {
            return format!(".{}", classes.join("."));
        } else {
            return format!(".{}", rest.trim());
        }
    }

    // Handle id. prefix
    if selector.starts_with("id.") {
        return format!("#{}", &selector[3..]);
    }

    // Handle tag. prefix
    if selector.starts_with("tag.") {
        return selector[4..].to_string();
    }

    // Already CSS selector format (index will be stripped in parse_selector_with_index)
    if selector.starts_with('.') {
        return selector.to_string();
    }

    if selector.starts_with('#') || selector.starts_with('[') {
        return selector.to_string();
    }

    selector.to_string()
}

fn parse_selector_with_index(selector: &str) -> ParsedSelector {
    let selector = selector.trim();

    if let Some((base, index_mode, index_items)) = parse_bracket_index_spec(selector) {
        return ParsedSelector {
            base: parse_selector_base(base),
            explicit_index: true,
            index_mode,
            index_items,
        };
    }

    if let Some((base, index_mode, index_items)) = parse_legacy_index_spec(selector) {
        return ParsedSelector {
            base: parse_selector_base(base),
            explicit_index: true,
            index_mode,
            index_items,
        };
    }

    ParsedSelector {
        base: parse_selector_base(selector),
        explicit_index: false,
        index_mode: IndexMode::Select,
        index_items: Vec::new(),
    }
}

fn parse_selector_base(selector: &str) -> SelectorBase {
    let selector = selector.trim();
    if selector.is_empty() || selector == "children" {
        return SelectorBase::Children;
    }
    if let Some(text) = selector.strip_prefix("text.") {
        return SelectorBase::Text(text.trim().to_string());
    }
    SelectorBase::Css(legado_to_css(selector))
}

fn parse_bracket_index_spec(selector: &str) -> Option<(&str, IndexMode, Vec<IndexItem>)> {
    let selector = selector.trim();
    if !selector.ends_with(']') {
        return None;
    }
    let start = selector.rfind('[')?;
    let base = selector[..start].trim();
    let mut inner = selector[start + 1..selector.len() - 1].trim();
    let index_mode = if let Some(rest) = inner.strip_prefix('!') {
        inner = rest.trim();
        IndexMode::Exclude
    } else {
        IndexMode::Select
    };

    let mut items = Vec::new();
    if !inner.is_empty() {
        for part in inner.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            if let Some(item) = parse_bracket_index_item(part) {
                items.push(item);
            } else {
                return None;
            }
        }
    }

    Some((base, index_mode, items))
}

fn parse_bracket_index_item(part: &str) -> Option<IndexItem> {
    if part.contains(':') {
        let segments: Vec<&str> = part.split(':').collect();
        if segments.len() < 2 || segments.len() > 3 {
            return None;
        }
        let start = parse_optional_i32(segments[0])?;
        let end = parse_optional_i32(segments[1])?;
        let step = match segments.get(2) {
            Some(value) => parse_optional_i32(value)?.unwrap_or(1),
            None => 1,
        };
        return Some(IndexItem::Range { start, end, step });
    }

    Some(IndexItem::Single(part.parse().ok()?))
}

fn parse_optional_i32(part: &str) -> Option<Option<i32>> {
    let part = part.trim();
    if part.is_empty() {
        return Some(None);
    }
    Some(Some(part.parse().ok()?))
}

fn parse_legacy_index_spec(selector: &str) -> Option<(&str, IndexMode, Vec<IndexItem>)> {
    for (delimiter, index_mode) in [('!', IndexMode::Exclude), ('.', IndexMode::Select)] {
        let Some(pos) = selector.rfind(delimiter) else {
            continue;
        };
        let base = selector[..pos].trim();
        let tail = selector[pos + 1..].trim();
        if tail.is_empty() {
            continue;
        }

        let mut items = Vec::new();
        for part in tail.split(':') {
            let part = part.trim();
            if part.is_empty() {
                return None;
            }
            items.push(IndexItem::Single(part.parse().ok()?));
        }
        return Some((base, index_mode, items));
    }
    None
}

fn collect_matches<'a>(doc: &'a Html, selector: &ParsedSelector) -> Vec<ElementRef<'a>> {
    let matches = match &selector.base {
        SelectorBase::Css(css_selector) => select_css(doc, css_selector),
        SelectorBase::Children => Vec::new(),
        SelectorBase::Text(text) => select_by_text_doc(doc, text),
    };
    apply_indices(matches, selector)
}

fn collect_matches_from_element<'a>(
    el: ElementRef<'a>,
    selector: &ParsedSelector,
) -> Vec<ElementRef<'a>> {
    let matches = match &selector.base {
        SelectorBase::Css(css_selector) => select_css_from_element(el, css_selector),
        SelectorBase::Children => child_elements(el),
        SelectorBase::Text(text) => select_by_text_from_element(el, text),
    };
    apply_indices(matches, selector)
}

fn select_css<'a>(doc: &'a Html, css_selector: &str) -> Vec<ElementRef<'a>> {
    match split_jsoup_pseudos(css_selector) {
        Some((base, filters)) => {
            let mut matches = select_css_plain(doc, &base);
            apply_jsoup_filters(&mut matches, &filters);
            matches
        }
        None => select_css_plain(doc, css_selector),
    }
}

fn select_css_from_element<'a>(el: ElementRef<'a>, css_selector: &str) -> Vec<ElementRef<'a>> {
    match split_jsoup_pseudos(css_selector) {
        Some((base, filters)) => {
            let mut matches = match parse_css_selector(&base) {
                Some(sel) => el.select(&sel).collect(),
                None => Vec::new(),
            };
            apply_jsoup_filters(&mut matches, &filters);
            matches
        }
        None => match parse_css_selector(css_selector) {
            Some(sel) => el.select(&sel).collect(),
            None => Vec::new(),
        },
    }
}

fn select_css_plain<'a>(doc: &'a Html, css_selector: &str) -> Vec<ElementRef<'a>> {
    let Some(sel) = parse_css_selector(css_selector) else { return vec![] };
    doc.select(&sel).collect()
}

/// JSoup 专有伪类：`scraper` 不支持，但书源大量使用。
///
/// 参考 JSoup `Selector` 文档。这里只列**需要后置过滤**的那些 —— 标准 CSS
/// 伪类（`:nth-child` / `:first-child` / `:not` / `:has` / `:is`）交给 `scraper`
/// 原生处理，语义已经一致。
#[derive(Clone, Debug, PartialEq)]
enum JsoupFilter {
    /// `:eq(n)` —— 在**同级元素**中的序号等于 n（0 基，JSoup 语义）。
    Eq(i32),
    /// `:gt(n)` —— 同级序号大于 n。
    Gt(i32),
    /// `:lt(n)` —— 同级序号小于 n。
    Lt(i32),
    /// `:first` / `:last` —— 结果集首/尾（JSoup 把这两个当作 `:eq(0)` / `:eq(-1)`
    /// 的同义写法，但作用在**结果集**上）。
    First,
    Last,
    /// `:contains(text)` —— 元素自身或后代的**规范化**文本包含 text（忽略大小写）。
    Contains(String),
    /// `:containsOwn(text)` —— 只看元素自身的直接文本。
    ContainsOwn(String),
    /// `:containsData(text)` —— script/style 等数据节点内容。
    ContainsData(String),
    /// `:containsWholeText(text)` —— 未规范化的整段文本，大小写敏感。
    ContainsWholeText(String),
    /// `:containsWholeOwnText(text)` —— 未规范化的自身文本，大小写敏感。
    ContainsWholeOwnText(String),
    /// `:matches(regex)` —— 规范化文本匹配正则。
    Matches(String),
    /// `:matchesOwn(regex)` —— 自身文本匹配正则。
    MatchesOwn(String),
    /// `:matchesWholeText(regex)` —— 未规范化整段文本匹配正则。
    MatchesWholeText(String),
    /// `:matchesWholeOwnText(regex)` —— 未规范化自身文本匹配正则。
    MatchesWholeOwnText(String),
    /// `:empty` —— 没有元素子节点且无有效文本。
    Empty,
}

/// 把选择器拆成「`scraper` 可解析的基础部分」+「需要自己过滤的伪类列表」。
///
/// 返回 `None` 表示没有 JSoup 专有伪类，调用方走原生路径（零开销）。
fn split_jsoup_pseudos(selector: &str) -> Option<(String, Vec<JsoupFilter>)> {
    let bytes = selector.as_bytes();
    let mut base = String::with_capacity(selector.len());
    let mut filters = Vec::new();
    let mut i = 0;
    let mut changed = false;

    while i < selector.len() {
        // 跳过属性选择器 `[...]`：里面的冒号不是伪类（如 [property=og:novel:x]）。
        if bytes[i] == b'[' {
            let mut depth = 0i32;
            while i < selector.len() {
                match bytes[i] {
                    b'[' => depth += 1,
                    b']' => {
                        depth -= 1;
                        if depth == 0 {
                            base.push(']');
                            i += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                // 必须按**字符**推进:选择器里可能含多字节字符(如中文属性值),
                // 用字节 `i += 1` 会切在字符中间并 panic。
                let ch = selector[i..].chars().next().unwrap();
                base.push(ch);
                i += ch.len_utf8();
            }
            continue;
        }
        if bytes[i] != b':' {
            let ch = selector[i..].chars().next().unwrap();
            base.push(ch);
            i += ch.len_utf8();
            continue;
        }

        let rest = &selector[i..];
        let Some((name, arg, consumed, has_arg)) = parse_pseudo(rest) else {
            base.push(':');
            i += 1;
            continue;
        };
        let filter = match name.as_str() {
            "eq" => Some(JsoupFilter::Eq(parse_int_arg(&arg)?)),
            "gt" => Some(JsoupFilter::Gt(parse_int_arg(&arg)?)),
            "lt" => Some(JsoupFilter::Lt(parse_int_arg(&arg)?)),
            "first" => Some(JsoupFilter::First),
            "last" => Some(JsoupFilter::Last),
            "contains" => Some(JsoupFilter::Contains(unescape_pseudo_text(&arg))),
            "containsOwn" => Some(JsoupFilter::ContainsOwn(unescape_pseudo_text(&arg))),
            "containsData" => Some(JsoupFilter::ContainsData(unescape_pseudo_text(&arg))),
            "containsWholeText" => Some(JsoupFilter::ContainsWholeText(unescape_pseudo_text(&arg))),
            "containsWholeOwnText" => {
                Some(JsoupFilter::ContainsWholeOwnText(unescape_pseudo_text(&arg)))
            }
            "matches" => Some(JsoupFilter::Matches(arg)),
            "matchesOwn" => Some(JsoupFilter::MatchesOwn(arg)),
            "matchesWholeText" => Some(JsoupFilter::MatchesWholeText(arg)),
            "matchesWholeOwnText" => Some(JsoupFilter::MatchesWholeOwnText(arg)),
            "empty" => Some(JsoupFilter::Empty),
            _ => None,
        };
        match filter {
            Some(f) => {
                filters.push(f);
                changed = true;
                i += consumed;
            }
            None => {
                // 标准伪类（:nth-child / :not / :has / :is 等）原样保留给 scraper。
                base.push_str(&rest[..consumed]);
                i += consumed;
                let _ = has_arg;
            }
        }
    }

    if !changed || filters.is_empty() {
        return None;
    }
    Some((base, filters))
}

/// 解析 `:name(arg)` / `:name`，返回 (名字, 参数, 消耗字节数, 是否有参数)。
fn parse_pseudo(rest: &str) -> Option<(String, String, usize, bool)> {
    let mut chars = rest.char_indices();
    chars.next()?; // ':'
    let mut name = String::new();
    let mut end = 0;
    for (idx, ch) in chars {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            name.push(ch);
            end = idx + ch.len_utf8();
        } else {
            break;
        }
    }
    if name.is_empty() {
        return None;
    }
    let after = &rest[end..];
    if !after.starts_with('(') {
        return Some((name, String::new(), end, false));
    }
    // 找配对的右括号（支持嵌套与转义）。
    let mut depth = 0i32;
    let mut escaped = false;
    let mut arg = String::new();
    let mut close = None;
    for (idx, ch) in after.char_indices() {
        if escaped {
            arg.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' => {
                arg.push(ch);
                escaped = true;
            }
            '(' => {
                depth += 1;
                if depth > 1 {
                    arg.push(ch);
                }
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(idx);
                    break;
                }
                arg.push(ch);
            }
            _ => arg.push(ch),
        }
    }
    let close = close?;
    Some((name, arg, end + close + 1, true))
}

fn parse_int_arg(arg: &str) -> Option<i32> {
    arg.trim().parse::<i32>().ok()
}

/// JSoup 允许在文本参数里用 `\(` `\)` 转义括号。
fn unescape_pseudo_text(arg: &str) -> String {
    arg.replace("\\(", "(").replace("\\)", ")")
}

/// 依次应用 JSoup 伪类过滤。
fn apply_jsoup_filters<'a>(matches: &mut Vec<ElementRef<'a>>, filters: &[JsoupFilter]) {
    for filter in filters {
        match filter {
            // `:eq/:gt/:lt` 在 JSoup 里看的是**同级序号**（sibling index），
            // 不是结果集下标。这一点很容易搞错：`td:eq(0)` 是"每行的第一个 td"。
            JsoupFilter::Eq(n) => {
                matches.retain(|el| eq_matches(el, *n));
            }
            JsoupFilter::Gt(n) => matches.retain(|el| (sibling_index(el) as i32) > *n),
            JsoupFilter::Lt(n) => matches.retain(|el| (sibling_index(el) as i32) < *n),
            JsoupFilter::First => matches.truncate(1),
            JsoupFilter::Last => {
                if matches.len() > 1 {
                    let last = matches.len() - 1;
                    matches.drain(..last);
                }
            }
            JsoupFilter::Contains(text) => {
                let needle = text.to_lowercase();
                matches.retain(|el| normalize_ws(&full_text(el)).to_lowercase().contains(&needle));
            }
            JsoupFilter::ContainsOwn(text) => {
                let needle = text.to_lowercase();
                matches.retain(|el| normalize_ws(&own_text(el)).to_lowercase().contains(&needle));
            }
            JsoupFilter::ContainsData(text) => {
                let needle = text.to_lowercase();
                matches.retain(|el| {
                    el.children().any(|node| {
                        node.value()
                            .as_text()
                            .map(|t| t.text.to_lowercase().contains(&needle))
                            .unwrap_or(false)
                    })
                });
            }
            JsoupFilter::ContainsWholeText(text) => {
                matches.retain(|el| full_text(el).contains(text.as_str()));
            }
            JsoupFilter::ContainsWholeOwnText(text) => {
                matches.retain(|el| own_text(el).contains(text.as_str()));
            }
            JsoupFilter::Matches(pattern) => {
                let Ok(re) = regex::Regex::new(pattern) else { continue };
                matches.retain(|el| re.is_match(&normalize_ws(&full_text(el))));
            }
            JsoupFilter::MatchesOwn(pattern) => {
                let Ok(re) = regex::Regex::new(pattern) else { continue };
                matches.retain(|el| re.is_match(&normalize_ws(&own_text(el))));
            }
            JsoupFilter::MatchesWholeText(pattern) => {
                let Ok(re) = regex::Regex::new(pattern) else { continue };
                matches.retain(|el| re.is_match(&full_text(el)));
            }
            JsoupFilter::MatchesWholeOwnText(pattern) => {
                let Ok(re) = regex::Regex::new(pattern) else { continue };
                matches.retain(|el| re.is_match(&own_text(el)));
            }
            JsoupFilter::Empty => {
                matches.retain(|el| {
                    el.children().all(|node| {
                        node.value()
                            .as_text()
                            .map(|t| t.text.trim().is_empty())
                            .unwrap_or(false)
                    })
                });
            }
        }
    }
}

/// `:eq(n)` 判定。JSoup 的 `:eq(-1)` 表示倒数第一个同级元素。
fn eq_matches(el: &ElementRef, n: i32) -> bool {
    let index = sibling_index(el) as i32;
    if n >= 0 {
        return index == n;
    }
    let Some(parent) = el.parent().and_then(ElementRef::wrap) else {
        return false;
    };
    let total = parent
        .children()
        .filter(|node| node.value().is_element())
        .count() as i32;
    index == total + n
}

/// 元素在父节点中的元素序号(0 基)。无父节点时为 0。
fn sibling_index(el: &ElementRef) -> usize {
    let Some(parent) = el.parent().and_then(ElementRef::wrap) else {
        return 0;
    };
    parent
        .children()
        .filter(|node| node.value().is_element())
        .position(|node| node.id() == el.id())
        .unwrap_or(0)
}

/// 元素及其后代的全部文本（JSoup `Element.text()` 语义：空白规范化）。
fn full_text(el: &ElementRef) -> String {
    normalize_ws(&el.text().collect::<String>())
}

/// 把连续空白折叠成单个空格并去掉首尾空白。
fn normalize_ws(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !prev_space && !out.is_empty() {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out.trim_end().to_string()
}

fn parse_css_selector(selector: &str) -> Option<Selector> {    if let Ok(parsed) = Selector::parse(selector) { return Some(parsed); }
    // Jsoup accepts values such as [property=og:novel:book_name] without
    // quotes. Quote these values before handing the selector to scraper.
    static ATTR: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = ATTR.get_or_init(|| regex::Regex::new(r#"(\[[^\]\s=~|^$*]+[~|^$*]?=)([^"'\]\s]+)(\])"#).unwrap());
    let normalized = re.replace_all(selector, "${1}\"${2}\"${3}");
    Selector::parse(&normalized).ok()
}

fn child_elements<'a>(el: ElementRef<'a>) -> Vec<ElementRef<'a>> {
    el.children().filter_map(ElementRef::wrap).collect()
}

fn select_by_text_doc<'a>(doc: &'a Html, needle: &str) -> Vec<ElementRef<'a>> {
    let sel = Selector::parse("*").unwrap();
    doc.select(&sel)
        .filter(|el| own_text(el).contains(needle))
        .collect()
}

fn select_by_text_from_element<'a>(el: ElementRef<'a>, needle: &str) -> Vec<ElementRef<'a>> {
    let mut matches = Vec::new();
    if own_text(&el).contains(needle) {
        matches.push(el);
    }
    if let Ok(sel) = Selector::parse("*") {
        matches.extend(
            el.select(&sel)
                .filter(|candidate| own_text(candidate).contains(needle)),
        );
    }
    matches
}

fn own_text(el: &ElementRef) -> String {
    let mut text = String::new();
    for node in el.children() {
        if let Some(text_node) = node.value().as_text() {
            text.push_str(text_node.text.trim());
        }
    }
    text
}

fn apply_indices<'a>(
    matches: Vec<ElementRef<'a>>,
    selector: &ParsedSelector,
) -> Vec<ElementRef<'a>> {
    if !selector.explicit_index {
        return matches;
    }

    let resolved = resolve_indices(matches.len(), &selector.index_items);
    if selector.index_mode == IndexMode::Exclude {
        let exclude_set: HashSet<usize> = resolved.into_iter().collect();
        return matches
            .into_iter()
            .enumerate()
            .filter(|(idx, _)| !exclude_set.contains(idx))
            .map(|(_, el)| el)
            .collect();
    }

    resolved
        .into_iter()
        .filter_map(|idx| matches.get(idx).copied())
        .collect()
}

fn resolve_indices(len: usize, items: &[IndexItem]) -> Vec<usize> {
    if len == 0 {
        return Vec::new();
    }

    let mut seen = HashSet::new();
    let mut resolved = Vec::new();
    for item in items {
        for idx in expand_index_item(len, item) {
            if seen.insert(idx) {
                resolved.push(idx);
            }
        }
    }
    resolved
}

fn expand_index_item(len: usize, item: &IndexItem) -> Vec<usize> {
    match item {
        IndexItem::Single(idx) => normalize_index(*idx, len).into_iter().collect(),
        IndexItem::Range { start, end, step } => expand_range(len, *start, *end, *step),
    }
}

fn normalize_index(index: i32, len: usize) -> Option<usize> {
    let len_i32 = len as i32;
    let resolved = if index < 0 { len_i32 + index } else { index };
    if resolved < 0 || resolved >= len_i32 {
        return None;
    }
    Some(resolved as usize)
}

fn expand_range(len: usize, start: Option<i32>, end: Option<i32>, step: i32) -> Vec<usize> {
    if len == 0 {
        return Vec::new();
    }

    let len_i32 = len as i32;
    let mut start = start.unwrap_or(0);
    let mut end = end.unwrap_or(len_i32 - 1);

    if start < 0 {
        start += len_i32;
    }
    if end < 0 {
        end += len_i32;
    }

    if (start < 0 && end < 0) || (start >= len_i32 && end >= len_i32) {
        return Vec::new();
    }

    start = start.clamp(0, len_i32 - 1);
    end = end.clamp(0, len_i32 - 1);

    let step = if step > 0 {
        step as usize
    } else if -step < len_i32 {
        (step + len_i32).max(1) as usize
    } else {
        1
    };

    let mut indices = Vec::new();
    if start <= end {
        let mut current = start as usize;
        let end = end as usize;
        while current <= end {
            indices.push(current);
            current = match current.checked_add(step) {
                Some(next) => next,
                None => break,
            };
        }
    } else {
        let mut current = start as usize;
        let end = end as usize;
        loop {
            indices.push(current);
            if current <= end || current < step {
                break;
            }
            current -= step;
        }
    }

    indices
}

/// Select elements with Legado rule syntax
pub fn select_list<'a>(doc: &'a Html, selector: &str) -> Vec<ElementRef<'a>> {
    let selector = selector.trim();
    if split_top_level(selector, &["&&", "||", "%%"]).delimiter.is_some() {
        return select_with_combination(doc, selector);
    }
    let parts = split_top_level(selector, &["@"]).parts;
    let Some(first) = parts.first() else { return vec![] };
    // 空选择器 = "当前元素自身"（legado `element.select("")` 返回元素自身）。
    //
    // 调用方**只在规则里还有提取器时**才该走到这里（`select_text_list` 把
    // `@href` 拆成 selector="" + extractor="href" 后传空串进来）。完全空的规则
    // 由调用方提前拦掉 —— 否则 `nextTocUrl` 未配置时传空串会凭空产出元素内容。
    let mut matches = if first.trim().is_empty() {
        fragment_root(doc).into_iter().collect()
    } else {
        collect_matches(doc, &parse_selector_with_index(first))
    };
    for part in parts.iter().skip(1) {
        let parsed = parse_selector_with_index(part);
        matches = matches.into_iter()
            .flat_map(|el| collect_matches_from_element(el, &parsed)).collect();
    }
    matches
}

/// 片段文档的"根元素"：`body` 恰好只有一个元素子节点时返回它。
///
/// `parse_document` 会把任意 HTML 片段包成 `html > body > 片段`。当调用方交进来
/// 的是**单个元素**的 outerHtml（字段级规则就是这么做的），body 的唯一元素子节点
/// 就是"当前元素"。文档级规则的原点因此不是 `html`，而是这个元素 —— 对齐 legado
/// `element.select("")` 返回元素自身的语义。
///
/// 完整页面（body 有多个子节点）返回 `None`：那种情况下不存在"当前元素"，
/// 与 legado 里根 document 没有 `href` 一致，避免凭空返回第一个 div。
fn fragment_root(doc: &Html) -> Option<ElementRef<'_>> {
    let body_sel = Selector::parse("body").ok()?;
    let body = doc.select(&body_sel).next()?;
    let mut children = body.children().filter_map(ElementRef::wrap);
    let first = children.next()?;
    if children.next().is_some() {
        return None;
    }
    Some(first)
}

/// Handle list combination operators
fn select_with_combination<'a>(doc: &'a Html, rule: &str) -> Vec<ElementRef<'a>> {
    let split = split_top_level(rule, &["&&", "||", "%%"]);
    let rules = split.parts;

    if rules.is_empty() {
        return vec![];
    }

    let mut result = select_list(doc, &rules[0]);
    let operator = split.delimiter.as_deref().unwrap_or("");

    for next_rule in rules.iter().skip(1) {
        let next_results = select_list(doc, next_rule);

        match operator {
            "&&" => {
                result.extend(next_results);
            }
            "||" => {
                if result.is_empty() {
                    result = next_results;
                }
            }
            "%%" => {
                let mut zipped = Vec::new();
                let max_len = result.len().max(next_results.len());
                for j in 0..max_len {
                    if j < result.len() {
                        zipped.push(result[j]);
                    }
                    if j < next_results.len() {
                        zipped.push(next_results[j]);
                    }
                }
                result = zipped;
            }
            _ => {}
        }
    }

    result
}

/// Extract text from element with various Legado extractors
pub fn extract_text(el: &ElementRef, extractor: &str) -> Option<String> {
    let extractor = extractor.trim();

    match extractor {
        "text" | "@text" => {
            let text = el.text().collect::<Vec<_>>().join(" ");
            let text = text.trim().to_string();
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        }
        "textNodes" | "@textNodes" => {
            let text = get_text_nodes(el);
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        }
        "ownText" | "@ownText" => {
            let mut own_text = String::new();
            for node in el.children() {
                if let Some(text_node) = node.value().as_text() {
                    own_text.push_str(text_node.text.trim());
                    own_text.push(' ');
                }
            }
            let text = own_text.trim().to_string();
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        }
        "html" | "@html" => Some(element_outer_html_without_scripts(el)),
        "all" | "@all" => Some(el.html()),
        _ => {
            if let Some(attr_name) = parse_attr_extractor(extractor) {
                return el.value().attr(attr_name).map(|v| v.to_string());
            }

            if extractor.starts_with('@') {
                el.value().attr(&extractor[1..]).map(|v| v.to_string())
            } else {
                el.value().attr(extractor).map(|v| v.to_string())
            }
        }
    }
}

/// legado `AnalyzeByJSoup.kt` 的 `@html` 语义: 先移除 `script` / `style` 再取 outerHtml。
/// 正文 HTML 会直通前端渲染, 不移除会让 `<style>` 污染阅读器样式、`<script>` 干扰插件替换。
fn element_outer_html_without_scripts(el: &ElementRef) -> String {
    let original = el.html();
    // 绝大多数正文没有 script/style, 直接返回, 不必重建树
    if !original.contains("<script") && !original.contains("<style") {
        return original;
    }
    let Ok(selector) = Selector::parse("script, style") else {
        return original;
    };
    let fragment = Html::parse_fragment(&original);
    let ids: Vec<_> = fragment
        .select(&selector)
        .map(|matched| matched.id())
        .collect();
    if ids.is_empty() {
        return original;
    }
    let mut tree = fragment.tree;
    for id in ids {
        if let Some(mut node) = tree.get_mut(id) {
            node.detach();
        }
    }
    // parse_fragment 会额外包一层根元素, 取其 inner_html 即为原始 outerHtml 去掉 script/style
    tree.root()
        .first_child()
        .and_then(ElementRef::wrap)
        .map(|wrapper| wrapper.inner_html())
        .unwrap_or(original)
}

fn parse_attr_extractor(extractor: &str) -> Option<&str> {    let extractor = extractor.trim();
    let extractor = extractor.strip_prefix('@').unwrap_or(extractor);
    extractor
        .strip_prefix("attr[")
        .and_then(|s| s.strip_suffix(']'))
        .filter(|s| !s.is_empty())
}

/// Get all text nodes from an element, preserving structure
fn get_text_nodes(el: &ElementRef) -> String {
    let mut texts = Vec::new();
    collect_text_nodes(*el, &mut texts);
    texts.join("\n")
}

fn collect_text_nodes(el: ElementRef, texts: &mut Vec<String>) {
    for node in el.children() {
        if let Some(text_node) = node.value().as_text() {
            let text = text_node.text.trim().to_string();
            if !text.is_empty() {
                texts.push(text);
            }
        }
        if let Some(child_el) = ElementRef::wrap(node) {
            collect_text_nodes(child_el, texts);
        }
    }
}

pub fn select_text_from_element(el: &ElementRef, rule: &str) -> Option<String> {
    let parts = split_top_level(rule, &["@"]).parts;
    let mut current_matches = vec![*el];

    for i in 0..parts.len() {
        let part = parts[i].trim();
        if part.is_empty() {
            continue;
        }

        if i == parts.len() - 1 {
            return current_matches
                .into_iter()
                .find_map(|current| extract_text(&current, part));
        }

        let parsed = parse_selector_with_index(part);
        let current_level = current_matches;
        let mut next_matches = Vec::new();
        for current in current_level {
            next_matches.extend(collect_matches_from_element(current, &parsed));
        }
        if next_matches.is_empty() {
            return None;
        }
        current_matches = next_matches;
    }

    current_matches
        .into_iter()
        .find_map(|current| extract_text(&current, "text"))
}

/// Select all matching elements and collect their text, joined by newlines.
///
/// 多级链式(`class.pic.0@tag.img@src`)必须**逐级下钻**: 早先这里把 `@` 后的
/// 整段当成"一个提取器", 多级规则全部取不到值 —— 某书源详情页
/// `coverUrl = {{@@class.pic.0@tag.img@src}}` 因此恒为空, 详情页没有封面
/// (列表页的 coverUrl 走 element 级求值, 不受影响)。
///
/// 无提取器的单段规则(`class.content`)保持原有 `textNodes` 语义: 文本节点按
/// **换行**拼接, 正文段落靠它保留换行, 不能退化成空格拼接。
pub fn select_all_text(doc: &Html, rule: &str) -> Option<String> {
    let parts = split_top_level(rule, &["@"]).parts;
    if parts.is_empty() {
        return None;
    }

    let first_part = parts[0].trim();
    // 空选择器 = 当前元素自身，但仅当后面跟着提取器时（`@href` / `@text`）。
    let roots: Vec<ElementRef<'_>> = if first_part.is_empty() && parts.len() > 1 {
        fragment_root(doc).into_iter().collect()
    } else {
        collect_matches(doc, &parse_selector_with_index(first_part))
    };
    if roots.is_empty() {
        return None;
    }

    if parts.len() == 1 {
        let mut texts = Vec::new();
        for root in roots {
            if let Some(text) = extract_text(&root, "textNodes") {
                if !text.is_empty() {
                    texts.push(text);
                }
            }
        }
        if texts.is_empty() {
            return None;
        }
        return Some(texts.join("\n"));
    }

    let rest: Vec<String> = parts[1..].iter().map(|p| p.trim().to_string()).collect();
    let mut all_texts = Vec::new();
    for root in roots {
        collect_chain_texts(root, &rest, &mut all_texts);
    }
    if all_texts.is_empty() {
        return None;
    }
    Some(all_texts.join("\n"))
}

/// 逐级下钻 `sel@sel@...@末段`。末段先按提取器解释(`text` / `href` / `src` …),
/// 解释不通再当选择器取其文本 —— 与原实现的两段语义一致, 只是中间层可以有多级。
fn collect_chain_texts(el: ElementRef<'_>, parts: &[String], out: &mut Vec<String>) {
    let Some(last) = parts.last() else { return };
    let mut current = vec![el];
    for part in &parts[..parts.len() - 1] {
        if part.is_empty() {
            continue;
        }
        let parsed = parse_selector_with_index(part);
        let mut next = Vec::new();
        for node in current {
            next.extend(collect_matches_from_element(node, &parsed));
        }
        if next.is_empty() {
            return;
        }
        current = next;
    }
    for node in current {
        if let Some(text) = extract_text(&node, last) {
            if !text.is_empty() {
                out.push(text);
            }
            continue;
        }
        let parsed = parse_selector_with_index(last);
        for matched in collect_matches_from_element(node, &parsed) {
            if let Some(text) = extract_text(&matched, "text") {
                if !text.is_empty() {
                    out.push(text);
                }
            }
        }
    }
}

pub fn select_text(doc: &Html, rule: &str) -> Option<String> {
    select_text_list(doc, rule).into_iter().next()
}

pub fn select_text_list(doc: &Html, rule: &str) -> Vec<String> {
    let combo = split_top_level(rule, &["&&", "||", "%%"]);
    if let Some(operator) = combo.delimiter.as_deref() {
        let mut result =
            select_text_list(doc, combo.parts.first().map(String::as_str).unwrap_or(""));
        for part in combo.parts.iter().skip(1) {
            let next = select_text_list(doc, part);
            match operator {
                "&&" => result.extend(next),
                "||" => {
                    if result.is_empty() {
                        result = next;
                    }
                }
                "%%" => {
                    let mut zipped = Vec::new();
                    let max_len = result.len().max(next.len());
                    for idx in 0..max_len {
                        if idx < result.len() {
                            zipped.push(result[idx].clone());
                        }
                        if idx < next.len() {
                            zipped.push(next[idx].clone());
                        }
                    }
                    result = zipped;
                }
                _ => {}
            }
        }
        return result;
    }

    // Handle rule chaining with @@
    if rule.contains("@@") {
        let rules: Vec<&str> = rule.split("@@").collect();
        if rules.is_empty() {
            return vec![];
        }

        // Start with first rule - get all matching texts
        let mut current_texts = select_text_list(doc, rules[0]);

        // Apply subsequent rules
        for r in rules.iter().skip(1) {
            if current_texts.is_empty() {
                break;
            }
            let mut new_texts = Vec::new();
            for text in &current_texts {
                let sub_doc = Html::parse_document(text);
                new_texts.extend(select_text_list(&sub_doc, r));
            }
            current_texts = new_texts;
        }

        return current_texts;
    }

    let parts = split_top_level(rule, &["@"]).parts;
    if parts.is_empty() {
        return vec![];
    }

    let (selector, extractor) = if parts.len() > 1 {
        (parts[..parts.len()-1].join("@"), parts.last().unwrap().as_str())
    } else {
        (parts[0].clone(), "text")
    };
    // 完全空的规则直接返回空：不能交给 `select_list`，那里空选择器表示"当前元素
    // 自身"（`@href` 的形态），会让空规则凭空产出元素内容。
    if selector.trim().is_empty() && parts.len() == 1 {
        return vec![];
    }
    select_list(doc, &selector).into_iter()
        .filter_map(|el| extract_text(&el, extractor)).collect()
}

/// XPath support using sxd-xpath
/// 把 legado/JsoupXpath 的 `...::text`、`...::ownText`、`...::html` 等轴扩展
/// 拆成 (XPath 路径, 提取器)。返回的路径可供标准 XPath 1.0 解析。
/// `pub` 供 rule_engine 的节点级 XPath 求值复用。
/// legado/JsoupXpath 的轴扩展提取器集合。`split_xpath_axis` 只认这些名字,
/// 避免把标准 XPath 1.0 轴(`child::span` / `parent::node` / `descendant::div`
/// 等)误当成提取器拆掉, 破坏合法 XPath 语义。
const LEGADO_XPATH_EXTRACTORS: &[&str] = &[
    "text",
    "ownText",
    "html",
    "outerHtml",
    "allText",
    "textNodes",
    "node",
];

/// 标准 XPath 1.0 轴名集合。`::` 前是这些名字时, `::word` 是轴限定(node-test),
/// 不是 legado 提取器, 不应拆分(如 `child::span` / `parent::node`)。
const XPATH_AXIS_NAMES: &[&str] = &[
    "ancestor",
    "ancestor-or-self",
    "attribute",
    "child",
    "descendant",
    "descendant-or-self",
    "following",
    "following-sibling",
    "namespace",
    "parent",
    "preceding",
    "preceding-sibling",
    "self",
];

pub fn split_xpath_axis(xpath: &str) -> (String, Option<&str>) {
    // 形如 `//div[@class='x']::text` → 路径 + 提取器 `text`
    // 注意避开 XPath 合法语法中的 `::`(轴限定, 如 `child::span`):
    // 只在末尾 `::word` 且 word 属于 legado 已知提取器集合、且 `::` 前不是
    // 标准 XPath 轴名时, 才拆分。后者用于消歧 `parent::node` 这种 `::` 前
    // 是轴名的情况(node 既是 legado 提取器又是 XPath 节点测试)。
    let trimmed = xpath.trim();
    // 匹配末尾 `::extractor`(extractor 为字母)
    let len = trimmed.len();
    let bytes = trimmed.as_bytes();
    let mut start = len;
    for &b in bytes.iter().rev() {
        if b.is_ascii_alphabetic() {
            start -= 1;
        } else {
            break;
        }
    }
    // 左边必须是 `::`
    if start < 2 {
        return (trimmed.to_string(), None);
    }
    let left = &trimmed[..start];
    if !left.ends_with("::") {
        return (trimmed.to_string(), None);
    }
    let extractor = &trimmed[start..];
    // 只接受 legado 已知提取器; 否则视为标准 XPath 轴, 原样返回。
    if !LEGADO_XPATH_EXTRACTORS.contains(&extractor) {
        return (trimmed.to_string(), None);
    }
    // `::` 前是标准 XPath 轴名 → 这是轴限定(node-test), 不是 legado 提取器。
    // 取 `::` 前最后一段(按 `/` 分割)判断轴名。
    let before = left[..left.len() - 2].trim_end();
    let last_segment = before.rsplit('/').next().unwrap_or(before);
    if XPATH_AXIS_NAMES.contains(&last_segment) {
        return (trimmed.to_string(), None);
    }
    let path = before.trim().to_string();
    (path, Some(extractor))
}

/// 递归把 sxd_xpath 节点序列化为 HTML 字符串(供 `::html` / `::outerHtml`)
/// 递归序列化 sxd 节点为 HTML 字符串(含外层标签)。
fn node_to_html_inner(node: sxd_xpath::nodeset::Node<'_>, text: &mut String) {
    use sxd_xpath::nodeset::Node;
    match node {
        Node::Element(el) => {
            let name = el.name().local_part();
            text.push('<');
            text.push_str(name);
            for attr in el.attributes() {
                let a = attr.name();
                text.push(' ');
                text.push_str(a.local_part());
                text.push_str("=\"");
                text.push_str(&attr.value().replace('"', "&quot;"));
                text.push('"');
            }
            text.push('>');
            for child in el.children() {
                use sxd_document::dom::ChildOfElement;
                match child {
                    // 子元素 → 递归
                    ChildOfElement::Element(e) => {
                        node_to_html_inner(Node::Element(e), text);
                    }
                    ChildOfElement::Text(t) => {
                        text.push_str(
                            &t.text()
                                .replace('&', "&amp;")
                                .replace('<', "&lt;")
                                .replace('>', "&gt;"),
                        );
                    }
                    ChildOfElement::Comment(c) => {
                        text.push_str("<!--");
                        text.push_str(c.text());
                        text.push_str("-->");
                    }
                    ChildOfElement::ProcessingInstruction(pi) => {
                        if let Some(v) = pi.value() {
                            text.push_str(&format!("<?{} {}?>", pi.target(), v));
                        }
                    }
                }
            }
            text.push_str("</");
            text.push_str(name);
            text.push('>');
        }
        Node::Text(t) => {
            text.push_str(
                &t.text()
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;"),
            );
        }
        Node::Comment(c) => {
            text.push_str("<!--");
            text.push_str(c.text());
            text.push_str("-->");
        }
        Node::ProcessingInstruction(pi) => {
            if let Some(v) = pi.value() {
                text.push_str(&format!("<?{} {}?>", pi.target(), v));
            }
        }
        _ => {}
    }
}

/// 序列化节点为 outer HTML(含外层标签)。
fn node_to_html(node: &sxd_xpath::nodeset::Node<'_>) -> String {
    let mut out = String::new();
    node_to_html_inner(*node, &mut out);
    out
}

/// 序列化节点的子节点(inner HTML), 不含外层标签本身。
/// Legado 中 `::html` 返回 inner HTML, `::outerHtml` 返回含外层标签的完整序列化。
fn node_inner_html(node: &sxd_xpath::nodeset::Node<'_>) -> String {
    let mut out = String::new();
    for child in node.children() {
        node_to_html_inner(child, &mut out);
    }
    out
}

/// 从 sxd 节点按提取器取出文本/html。提取器为 legado/JsoupXpath 轴扩展名。
pub fn node_extract(node: &sxd_xpath::nodeset::Node<'_>, extractor: &str) -> Option<String> {
    use sxd_xpath::nodeset::Node;
    match extractor {
        // ::text / ::textNodes / ::allText / ::node → 元素树文本
        "text" | "textNodes" | "allText" | "node" => {
            let v = node.string_value();
            if v.trim().is_empty() {
                None
            } else {
                Some(v)
            }
        }
        // ::ownText → 仅直接文本子节点
        "ownText" => {
            let mut own = String::new();
            for child in node.children() {
                if let Node::Text(t) = child {
                    own.push_str(t.text().trim());
                    own.push(' ');
                }
            }
            let v = own.trim().to_string();
            if v.is_empty() {
                None
            } else {
                Some(v)
            }
        }
        // ::html → inner HTML (子节点序列化, 不含外层标签)
        "html" => {
            let v = node_inner_html(node);
            if v.trim().is_empty() {
                None
            } else {
                Some(v)
            }
        }
        // ::outerHtml → outer HTML (含外层标签)
        "outerHtml" => {
            let v = node_to_html(node);
            if v.trim().is_empty() {
                None
            } else {
                Some(v)
            }
        }
        _ => None,
    }
}

/// 把 HTML 转成 `sxd_document`(XML 解析器)能吃的良构文档, 供 XPath 求值。
///
/// `sxd_document` 是**严格 XML** 解析器, 而真实的书源页面几乎都不是良构 XML:
/// 未闭合的 `<br>`/`<img>`/`<meta>`、重复属性、未加引号的属性值、裸 `&`、
/// `<script>` 里的 `<` 与 `&&`……直接喂原始 HTML 会整份解析失败返回空,
/// 结果就是「XPath 写法的发现页/目录规则一条都取不到」。
///
/// 正确做法是先按 HTML5 规则建 DOM(`scraper`/html5ever, 与 CSS 路径同一棵树),
/// 再序列化成良构 XML。这一步与 legado 的行为对齐: 手机端用的是
/// `org.jsoup` + `jsoupXpath`, 同样先把 HTML 解析成 DOM 再跑 XPath。
///
/// 两点细节:
/// - 同名属性(如 `src` 与 `srcset` 之外还有重复的 `alt`)在 XML 里非法,
///   `scraper` 的 `attrs()` 迭代器会重复吐同一个键, 必须按名去重;
/// - 非法属性名(数字开头等)整条丢弃, 宁可少一个属性也不要让整份文档解析失败。
pub fn html_to_xml(html: &str) -> String {
    const VOID: &[&str] = &[
        "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param",
        "source", "track", "wbr",
    ];
    fn escape_text(text: &str) -> String {
        text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
    }
    fn escape_attr(value: &str) -> String {
        value
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    }
    fn valid_name(name: &str) -> bool {
        let mut chars = name.chars();
        match chars.next() {
            Some(ch) if ch.is_ascii_alphabetic() || ch == '_' => {}
            _ => return false,
        }
        chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | ':'))
    }
    fn write_element(el: ElementRef<'_>, out: &mut String) {
        let name = el.value().name();
        out.push('<');
        out.push_str(name);
        let mut seen = HashSet::new();
        for (key, value) in el.value().attrs() {
            if !valid_name(key) || !seen.insert(key) {
                continue;
            }
            out.push(' ');
            out.push_str(key);
            out.push_str("=\"");
            out.push_str(&escape_attr(value));
            out.push('"');
        }
        if VOID.contains(&name) {
            out.push_str(" />");
            return;
        }
        out.push('>');
        for child in el.children() {
            match ElementRef::wrap(child) {
                Some(child) => write_element(child, out),
                None => {
                    if let Some(text) = child.value().as_text() {
                        out.push_str(&escape_text(text));
                    }
                }
            }
        }
        out.push_str("</");
        out.push_str(name);
        out.push('>');
    }

    let document = parse_document(html);
    let mut out = String::with_capacity(html.len() + 256);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    write_element(document.root_element(), &mut out);
    out
}

pub fn select_xpath(html: &str, xpath: &str) -> Vec<String> {
    let (path, extractor) = split_xpath_axis(xpath);

    let xml = html_to_xml(html);
    let package = match sxd_document::parser::parse(&xml) {
        Ok(p) => p,
        Err(_) => return vec![],
    };

    let document = package.as_document();
    let context = sxd_xpath::Context::new();

    match sxd_xpath::Factory::new().build(&path) {
        Ok(Some(xpath_expr)) => match xpath_expr.evaluate(&context, document.root()) {
            Ok(value) => match value {
                sxd_xpath::Value::Nodeset(ns) => {
                    let nodes: Vec<_> = ns.document_order();
                    if let Some(ext) = extractor {
                        nodes
                            .iter()
                            .filter_map(|n| node_extract(n, ext))
                            .collect::<Vec<_>>()
                    } else {
                        nodes.iter().map(|n| n.string_value()).collect()
                    }
                }
                sxd_xpath::Value::String(s) => vec![s],
                sxd_xpath::Value::Number(n) => vec![n.to_string()],
                sxd_xpath::Value::Boolean(b) => vec![b.to_string()],
            },
            Err(_) => vec![],
        },
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 多级链式(`class.pic.0@tag.img@src`)必须逐级下钻后取末段提取器。
    /// 某书源详情页 `coverUrl` 模板就是这么写的; 早先的 select_all_text 把
    /// `@` 之后的整段当成"单个提取器", 多级规则恒为空 → 详情页无封面。
    #[test]
    fn test_select_all_text_multi_level_chain() {
        let doc = parse_document(r#"<html><body><div class="pic"><a href="/b/1.html"><img src="/bookimg/9.jpg"></a></div></body></html>"#);
        assert_eq!(
            select_all_text(&doc, "class.pic.0@tag.img@src").as_deref(),
            Some("/bookimg/9.jpg")
        );
        assert_eq!(
            select_all_text(&doc, "class.pic@tag.a@tag.img@src").as_deref(),
            Some("/bookimg/9.jpg")
        );
    }

    /// 无提取器的单段规则保持 `textNodes` 语义: 文本节点按换行拼接
    /// (正文段落靠它保留换行)。
    #[test]
    fn test_select_all_text_single_rule_keeps_text_nodes_newlines() {
        let doc = parse_document("<html><body><div class=\"content\">第一段<br>第二段</div></body></html>");
        let text = select_all_text(&doc, "class.content").unwrap_or_default();
        assert!(text.contains("第一段"), "got {text:?}");
        assert!(text.contains("第二段"), "got {text:?}");
    }

    #[test]
    fn test_legado_to_css() {
        assert_eq!(legado_to_css("class.mod block"), ".mod.block");
        assert_eq!(legado_to_css("class.test"), ".test");
        assert_eq!(legado_to_css("id.main"), "#main");
        assert_eq!(legado_to_css("tag.div"), "div");
    }

    #[test]
    fn test_parse_selector_with_index() {
        let parsed = parse_selector_with_index(".test.0");
        assert_eq!(parsed.base, SelectorBase::Css(".test".to_string()));
        assert!(parsed.explicit_index);
        assert_eq!(parsed.index_mode, IndexMode::Select);
        assert_eq!(parsed.index_items, vec![IndexItem::Single(0)]);

        let parsed = parse_selector_with_index(".test.-1");
        assert_eq!(parsed.base, SelectorBase::Css(".test".to_string()));
        assert_eq!(parsed.index_items, vec![IndexItem::Single(-1)]);

        let parsed = parse_selector_with_index(".test!0");
        assert_eq!(parsed.base, SelectorBase::Css(".test".to_string()));
        assert_eq!(parsed.index_mode, IndexMode::Exclude);
        assert_eq!(parsed.index_items, vec![IndexItem::Single(0)]);

        let parsed = parse_selector_with_index("div[-1, 1:3]");
        assert_eq!(parsed.base, SelectorBase::Css("div".to_string()));
        assert_eq!(
            parsed.index_items,
            vec![
                IndexItem::Single(-1),
                IndexItem::Range {
                    start: Some(1),
                    end: Some(3),
                    step: 1,
                },
            ]
        );
    }

    #[test]
    fn test_select_text_list_with_bracket_indices() {
        let doc = parse_document(
            r#"<div><a href="/1">A</a><a href="/2">B</a><a href="/3">C</a><a href="/4">D</a></div>"#,
        );

        assert_eq!(
            select_text_list(&doc, "a[1:2]@text"),
            vec!["B".to_string(), "C".to_string()]
        );
        assert_eq!(
            select_text_list(&doc, "a[!1,2]@text"),
            vec!["A".to_string(), "D".to_string()]
        );
        assert_eq!(
            select_text_list(&doc, "a[-1:0]@text"),
            vec![
                "D".to_string(),
                "C".to_string(),
                "B".to_string(),
                "A".to_string()
            ]
        );
    }

    #[test]
    fn test_extract_attr_bracket_syntax() {
        let doc = parse_document(r#"<div><a href="/book/1" data-id="abc">Book</a></div>"#);

        assert_eq!(
            select_text(&doc, "a@attr[href]"),
            Some("/book/1".to_string())
        );
        assert_eq!(
            select_text(&doc, "a@attr[data-id]"),
            Some("abc".to_string())
        );
        assert_eq!(select_text(&doc, "a@href"), Some("/book/1".to_string()));
    }

    #[test]
    fn test_xpath_axis_extensions() {
        // legado/JsoupXpath 轴扩展: `//div::text`、`::ownText`、`::html`
        let html = r#"<root><div class="x"><span>hello</span> <b>world</b></div><div class="y">seed</div></root>"#;

        // ::text → 元素树文本
        assert_eq!(
            select_xpath(html, "//div[@class='x']::text"),
            vec!["hello world".to_string()]
        );
        // 无提取器 → 路径直接原样
        assert_eq!(
            select_xpath(html, "//div[@class='y']"),
            vec!["seed".to_string()]
        );
        // ::html → 序列化
        let got_html = select_xpath(html, "//div[@class='x']::html");
        assert_eq!(got_html.len(), 1);
        assert!(got_html[0].contains("hello"));
        assert!(got_html[0].contains("<span>"));
        // ::ownText → 该 div 的直接文本只有空白(子元素都被更深层包裹), 允许为空
        let own = select_xpath(html, "//div[@class='y']::ownText");
        assert_eq!(own, vec!["seed".to_string()]);
    }

    #[test]
    fn test_split_xpath_axis_leaves_standard_xpath_axis_intact() {
        // 标准 XPath 1.0 轴(child::span / parent::node / descendant::div)
        // 不能被当成 legado 提取器拆掉, 否则路径被截断、解析失败。
        assert_eq!(
            split_xpath_axis("//div/child::span"),
            ("//div/child::span".to_string(), None)
        );
        // parent::node 中 node 既是 legado 提取器又是 XPath 节点测试,
        // 但 `::` 前是轴名 parent → 不拆
        assert_eq!(
            split_xpath_axis("//a/parent::node"),
            ("//a/parent::node".to_string(), None)
        );
        assert_eq!(
            split_xpath_axis("//div/descendant::div"),
            ("//div/descendant::div".to_string(), None)
        );
        assert_eq!(
            split_xpath_axis("//div/ancestor::div"),
            ("//div/ancestor::div".to_string(), None)
        );
        assert_eq!(
            split_xpath_axis("//div/following-sibling::a"),
            ("//div/following-sibling::a".to_string(), None)
        );
        assert_eq!(
            split_xpath_axis("//div/self::node"),
            ("//div/self::node".to_string(), None)
        );
        // 末尾不是已知提取器名 → 不拆
        assert_eq!(
            split_xpath_axis("//div::notARealExtractor"),
            ("//div::notARealExtractor".to_string(), None)
        );
        // 已知提取器仍正常拆分(`::` 前不是轴名)
        assert_eq!(
            split_xpath_axis("//div[@class='x']::text"),
            ("//div[@class='x']".to_string(), Some("text"))
        );
        assert_eq!(
            split_xpath_axis("//div::ownText"),
            ("//div".to_string(), Some("ownText"))
        );
        assert_eq!(
            split_xpath_axis("//div::html"),
            ("//div".to_string(), Some("html"))
        );
        assert_eq!(
            split_xpath_axis("//div::outerHtml"),
            ("//div".to_string(), Some("outerHtml"))
        );
        assert_eq!(
            split_xpath_axis("//div::allText"),
            ("//div".to_string(), Some("allText"))
        );
        assert_eq!(
            split_xpath_axis("//div::textNodes"),
            ("//div".to_string(), Some("textNodes"))
        );
        // `::` 前是 div(非轴名) → 拆成提取器 node
        assert_eq!(
            split_xpath_axis("//div::node"),
            ("//div".to_string(), Some("node"))
        );
        // 无 :: → 原样
        assert_eq!(split_xpath_axis("//div/span"), ("//div/span".to_string(), None));
        // 无内容
        assert_eq!(split_xpath_axis(""), ("".to_string(), None));
    }

    #[test]
    fn select_xpath_html_extractor_returns_inner_html() {
        // ::html 应返回 inner HTML(子节点序列化), 不含外层 div 标签
        let html = r#"<div><p>hello</p><span>world</span></div>"#;
        let result = select_xpath(html, "//div::html");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "<p>hello</p><span>world</span>");
    }

    #[test]
    fn select_xpath_outer_html_extractor_returns_outer_html() {
        // ::outerHtml 应返回含外层标签的完整序列化
        let html = r#"<div><p>hello</p></div>"#;
        let result = select_xpath(html, "//div::outerHtml");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "<div><p>hello</p></div>");
    }

    /// legado `@html`: 取 outerHtml 但先移除 script/style(AnalyzeByJSoup.kt:260-267)。
    #[test]
    fn html_extractor_removes_script_and_style() {
        let doc = Html::parse_document(
            r#"<div class="con"><style>.x{color:red}</style><script>var a='</p>';</script><p>正文</p></div>"#,
        );
        let el = doc.select(&Selector::parse("div.con").unwrap()).next().unwrap();

        let html = extract_text(&el, "html").unwrap();
        assert_eq!(html, r#"<div class="con"><p>正文</p></div>"#);

        // `@all` 保持 legado 语义: 不移除任何内容
        let all = extract_text(&el, "all").unwrap();
        assert!(all.contains("<script>"));
        assert!(all.contains("<style>"));
    }
}
