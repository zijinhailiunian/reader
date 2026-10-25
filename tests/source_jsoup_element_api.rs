//! 书源运行时兼容回归: JSoup 元素的可变语义与表格片段解析。
//!
//! 三个问题的现场(均来自线上书源, 站点信息已脱敏):
//! - 搜索能搜到但结果 0 条: `bookList` 交回 `<tr id="nr">` 元素,
//!   字段规则 `td:nth-of-type(1) a@text` 在快照上求值。快照还原成 HTML 后交给
//!   HTML5 解析器时, 没有 `<table>` 上下文的 `tr`/`td` 会被整段丢弃, 于是
//!   每个字段都取空, 每本书都被跳过。
//! - 搜索直接报 `cannot read property 'Elements' of undefined`:
//!   jsLib 用 `new org.jsoup.select.Elements()` / `new org.jsoup.nodes.Element('div')`
//!   手工拼搜索结果卡片, 桌面端只提供了顶层 `org.jsoup.Elements` 且返回空集合。
//! - 正文规则写 `var c = box.clone(); c.select('a,script,style').remove();`,
//!   元素没有宿主关系时 `remove()` 删不掉任何东西, 正文里的广告/脚本清不掉。

use reader_rust::model::book_source::book_source_from_value;
use reader_rust::parser::js::{eval_js, take_js_logs};
use reader_rust::parser::rule_engine::RuleEngine;
use serde_json::json;

/// 搜索结果页: 六列表格, 每行 `<tr id="nr">`。
const SEARCH_TABLE: &str = r#"<html><body><table class="grid">
<tr><th>文章名称</th><th>最新章节</th><th>作者</th><th>字数</th><th>更新</th><th>状态</th></tr>
<tr id="nr">
  <td class="odd"><a href="https://m.example.test/xs/1.html">第一本书</a></td>
  <td class="even"><a href="https://m.example.test/xs/1/10.html">第2章 起点</a></td>
  <td class="odd">作者甲</td><td class="even">1023K</td><td class="odd">26-09-14</td><td class="even">连载</td>
</tr>
<tr id="nr">
  <td class="odd"><a href="https://m.example.test/xs/2.html">第二本书</a></td>
  <td class="even"><a href="https://m.example.test/xs/2/20.html">第9章 终点</a></td>
  <td class="odd">作者乙</td><td class="even">2048K</td><td class="odd">26-09-15</td><td class="even">完本</td>
</tr>
</table></body></html>"#;

fn table_source() -> reader_rust::model::book_source::BookSource {
    book_source_from_value(json!({
        "bookSourceUrl": "https://m.example.test",
        "bookSourceName": "表格片段样例",
        "searchUrl": "https://m.example.test/search.php?key={{key}}",
        "ruleSearch": {
            "bookList": "@js:(function(){var J=Packages.org.jsoup.Jsoup;var doc=J.parse(String(result||''));return doc.select('tr#nr');})()",
            "name": "td:nth-of-type(1) a@text",
            "author": "td:nth-of-type(3)@text",
            "bookUrl": "td:nth-of-type(1) a@href",
            "lastChapter": "td:nth-of-type(2) a@text",
            "wordCount": "td:nth-of-type(4)@text"
        }
    }))
    .unwrap()
}

/// `tr` 快照上的字段规则必须能选中表头之外的单元格。
#[test]
fn row_snapshot_fields_resolve_all_columns() {
    let engine = RuleEngine::new().expect("rule engine");
    let source = table_source();
    let books = engine.search_books(&source, SEARCH_TABLE, "https://m.example.test/search.php?key=x");
    assert_eq!(books.len(), 2, "两行结果都要解析出来");
    assert_eq!(books[0].name, "第一本书");
    assert_eq!(books[0].author, "作者甲");
    assert_eq!(books[0].book_url, "https://m.example.test/xs/1.html");
    assert_eq!(books[0].last_chapter.as_deref(), Some("第2章 起点"));
    assert_eq!(books[0].word_count.as_deref(), Some("1023K"));
    assert_eq!(books[1].name, "第二本书");
    assert_eq!(books[1].author, "作者乙");
}

/// 元素快照的 `select()` 也要能落在表格单元格上(`tr` 内部再查 `td`)。
#[test]
fn element_select_inside_table_row_keeps_cells() {
    let html = r#"<tr id="nr"><td>甲</td><td><a href="/1.html">书名</a></td></tr>"#;
    let out = eval_js(
        r#"
        var doc = org.jsoup.Jsoup.parse(String(result || ''));
        var tr = doc.select('tr').first();
        var cells = tr.select('td');
        var link = tr.select('td a').first();
        JSON.stringify([cells.size(), link ? String(link.text()) : '']);
        "#,
        html,
        "https://m.example.test/",
    )
    .expect("eval");
    assert_eq!(out, r#"[2,"书名"]"#);
}

/// `new org.jsoup.nodes.Element(tag)` + `appendChild` 拼出的卡片要能交回宿主。
#[test]
fn constructed_elements_serialize_into_search_items() {
    let out = eval_js(
        r#"
        var out = new org.jsoup.select.Elements();
        var div = new org.jsoup.nodes.Element('div');
        var h3 = new org.jsoup.nodes.Element('h3');
        var a = new org.jsoup.nodes.Element('a');
        a.attr('href', 'https://m.example.test/xs/9.html');
        a.text('拼出来的书');
        h3.appendChild(a);
        div.appendChild(h3);
        var span = new org.jsoup.nodes.Element('span');
        span.attr('class', 'au');
        span.text('拼出来的作者');
        div.appendChild(span);
        out.add(div);
        JSON.stringify({ count: out.size(), html: out.first().outerHtml() });
        "#,
        "",
        "https://m.example.test/",
    )
    .expect("eval");
    let value: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert_eq!(value["count"], 1);
    let html = value["html"].as_str().unwrap_or_default();
    assert!(html.contains("<div>"), "构造的元素要能序列化成 HTML: {html}");
    assert!(html.contains(r#"<a href="https://m.example.test/xs/9.html">拼出来的书</a>"#), "{html}");
    assert!(html.contains(r#"<span class="au">拼出来的作者</span>"#), "{html}");
}

/// 宿主把 `Elements` 交回时序列化成元素快照数组(每项都能套字段规则)。
#[test]
fn constructed_elements_snapshot_survives_json() {
    let out = eval_js(
        r#"
        var out = new org.jsoup.select.Elements();
        for (var i = 0; i < 2; i++) {
          var div = new org.jsoup.nodes.Element('div');
          var h3 = new org.jsoup.nodes.Element('h3');
          var a = new org.jsoup.nodes.Element('a');
          a.attr('href', '/book/' + i + '.html');
          a.text('书' + i);
          h3.appendChild(a);
          div.appendChild(h3);
          out.add(div);
        }
        JSON.stringify(out);
        "#,
        "",
        "https://m.example.test/",
    )
    .expect("eval");
    let items: Vec<serde_json::Value> = serde_json::from_str(&out).expect("json array");
    assert_eq!(items.len(), 2);
    for (index, item) in items.iter().enumerate() {
        assert!(
            item.get("outerHtml").and_then(|v| v.as_str()).is_some(),
            "第 {index} 项必须是元素快照: {item}"
        );
    }
}

/// `clone()` + `select(...).remove()` 要真的把子节点从副本里摘掉(正文清洗)。
#[test]
fn cloned_element_removes_selected_children() {
    let html = r#"<div class="content-body"><p>正文第一段</p><script>var ad=1;</script><a href="/ad">广告</a><p>正文第二段</p></div>"#;
    let out = eval_js(
        r#"
        var doc = org.jsoup.Jsoup.parse(String(result || ''));
        var box = doc.select('div.content-body').first();
        var copy = box.clone();
        copy.select('a,script,style').remove();
        String(copy.html());
        "#,
        html,
        "https://m.example.test/",
    )
    .expect("eval");
    assert!(out.contains("正文第一段"), "正文要保留: {out}");
    assert!(out.contains("正文第二段"), "正文要保留: {out}");
    assert!(!out.contains("广告"), "广告链接要删掉: {out}");
    assert!(!out.contains("var ad=1"), "脚本要删掉: {out}");
}

/// 元素用普通数组交出时必须仍是**扁平快照**。
///
/// 书源 `bookList` / 目录脚本常写 `var out = []; out.push(li); return out;`,
/// 宿主拿到的是 `JSON.stringify(out)`。元素若没有 `toJSON`(或内部字段可枚举),
/// 每项会变成 `{"__snapshot":{…}}` 包装, 宿主既认不出快照、字段规则也取不到值,
/// 表现就是「发现页/搜索页 0 条」。`Elements` 集合有 `elementsApi.toJSON` 兜底,
/// 裸数组只能靠元素自己的 `toJSON`。
#[test]
fn elements_pushed_into_plain_array_serialize_flat() {
    let html = r#"<ul class="list"><li><span class="s2"><a href="/book/1.html">书名</a></span></li></ul>"#;
    let out = eval_js(
        r#"
        var doc = org.jsoup.Jsoup.parse(String(result || ''));
        var lis = doc.select('ul.list li');
        var out = [];
        for (var i = 0; i < lis.size(); i++) out.push(lis.get(i));
        JSON.stringify(out);
        "#,
        html,
        "https://m.example.test/",
    )
    .expect("eval");
    let items: Vec<serde_json::Value> = serde_json::from_str(&out).expect("json array");
    assert_eq!(items.len(), 1);
    let item = &items[0];
    assert!(
        item.get("__snapshot").is_none(),
        "不能序列化成 __snapshot 包装: {item}"
    );
    assert_eq!(item["tag"], "li");
    assert!(
        item["outerHtml"].as_str().unwrap_or_default().contains("书名"),
        "快照要带 outerHtml: {item}"
    );
    assert!(
        item["attrs"].is_object(),
        "快照要带 attrs: {item}"
    );
}

/// 原文档在 `clone()` + `remove()` 之后不应被改动(JSoup 的 clone 是深拷贝)。
#[test]
fn clone_removal_does_not_touch_the_original_document() {
    let html = r#"<div class="content-body"><p>正文</p><a href="/ad">广告</a></div>"#;
    let out = eval_js(
        r#"
        var doc = org.jsoup.Jsoup.parse(String(result || ''));
        var box = doc.select('div.content-body').first();
        var copy = box.clone();
        copy.select('a').remove();
        JSON.stringify([String(copy.html()), String(box.html())]);
        "#,
        html,
        "https://m.example.test/",
    )
    .expect("eval");
    let value: Vec<String> = serde_json::from_str(&out).expect("json");
    assert!(!value[0].contains("广告"), "副本里的广告要删掉: {}", value[0]);
    assert!(value[1].contains("广告"), "原文档不能被动过: {}", value[1]);
}

/// 顶层 `org.jsoup.Elements` 别名仍可用(老源只写这个路径)。
#[test]
fn legacy_jsoup_aliases_still_construct_empty_collections() {
    take_js_logs();
    let out = eval_js(
        r#"
        var a = new org.jsoup.Elements();
        var b = new org.jsoup.select.Elements();
        var c = new org.jsoup.nodes.Element('p');
        JSON.stringify([a.size(), b.size(), String(c.tagName())]);
        "#,
        "",
        "https://m.example.test/",
    )
    .expect("eval");
    assert_eq!(out, r#"[0,0,"p"]"#);
}

/// 详情页字段(整页求值)也必须执行 `##匹配##替换`。
///
/// 书源的 `ruleBookInfo.author` 常写成
/// `div.info div.fix > p:nth-of-type(1)@text##作\s*者[：:]\s*`,
/// 而站点把「作者」写成 `作&nbsp;&nbsp;者：`(`&nbsp;` 折成 U+00A0, `\s` 在
/// Rust 正则里能吃掉它)。早先整页分支直接把 `##…` 当成选择器的一部分,
/// 选不到元素 → 作者/分类恒为空。
#[test]
fn detail_page_fields_apply_legado_regex_suffix() {
    let html = r#"<html><body>
        <div class="info">
          <div class="top">
            <h1>示例书名</h1>
            <div class="fix">
              <p>作&nbsp;&nbsp;者：示例作者</p>
              <p class="xs-show">类&nbsp;&nbsp;别：示例分类</p>
            </div>
          </div>
        </div>
    </body></html>"#;
    let source = book_source_from_value(json!({
        "bookSourceUrl": "https://m.example.test",
        "bookSourceName": "详情正则样例",
        "ruleBookInfo": {
            "name": "div.info h1@text",
            "author": "div.info div.fix > p:nth-of-type(1)@text##作\\s*者[：:]\\s*",
            "kind": "div.info div.fix > p:nth-of-type(2)@text##类\\s*别[：:]\\s*"
        }
    }))
    .expect("book source");
    let engine = RuleEngine::new().expect("rule engine");
    let base = "https://m.example.test/xs/1.html";
    let info = engine.book_info(&source, html, base, base);
    assert_eq!(info.name, "示例书名");
    assert_eq!(info.author, "示例作者");
    assert_eq!(info.kind.as_deref(), Some("示例分类"));
}
