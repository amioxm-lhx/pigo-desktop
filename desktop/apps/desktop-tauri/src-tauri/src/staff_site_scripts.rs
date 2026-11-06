//! JavaScript injected into the staff-site collector's hidden window.
//!
//! Each channel gets its own DOM selectors; the collector itself never talks to
//! a site API and the page never posts anywhere but the `pigo-collect://`
//! result scheme. Zhihu keeps its original scripts verbatim, while Juejin,
//! Bilibili, WeChat MP and WeChat Channels use the shared list/detail templates
//! with a channel config so the collectors cannot drift apart.

use serde_json::{json, Value};

const ZHIHU_LIST_SCRIPT: &str = r#"
(function () {
  var finished = false;
  function clean(value) {
    return (value || "").replace(/\s+/g, " ").trim();
  }
  function absolute(href) {
    try {
      var parsed = new URL(href, location.href);
      if (parsed.protocol !== "https:") return "";
      return parsed.href;
    } catch (_) {
      return "";
    }
  }
  function loginWall() {
    var path = location.pathname || "";
    if (/signin|signup|login/i.test(path)) return true;
    var sample = document.body ? document.body.innerText : "";
    return /扫码登录|登录\/注册|请先登录|密码登录/.test(sample);
  }
  function finish(payload) {
    if (finished) return;
    finished = true;
    try {
      payload.url = location.href;
      payload.path = location.pathname;
      payload.loginWall = loginWall();
      window.location.href = "pigo-collect://result?data=" +
        encodeURIComponent(JSON.stringify(payload));
    } catch (_) {}
  }
  function read() {
    var anchors = Array.from(document.querySelectorAll(
      'a[href*="/question/"], a[href*="zhuanlan.zhihu.com/p/"]'
    ));
    var seen = {};
    var items = [];
    for (var i = 0; i < anchors.length; i++) {
      var anchor = anchors[i];
      var url = absolute(anchor.getAttribute("href") || "");
      if (!url || seen[url]) continue;
      var titleNode = anchor.querySelector(
        "h2, .SearchResult-title, .HotItem-title, [class*='title']"
      );
      var title = clean(titleNode ? titleNode.textContent : anchor.textContent);
      if (!title) continue;
      var card = anchor.closest(
        ".SearchResult, .HotItem, .ContentItem, article, li"
      ) || anchor.parentElement;
      var excerpt = "";
      var meta = "";
      if (card && card !== anchor) {
        var body = card.querySelector(
          ".RichText, [class*='excerpt'], [class*='description'], [class*='summary']"
        );
        excerpt = clean(body ? body.textContent : "").slice(0, 240);
        var badge = card.querySelector(
          "[class*='vote'], [class*='Vote'], [class*='meta'], [class*='status']"
        );
        meta = clean(badge ? badge.textContent : "").slice(0, 80);
      }
      seen[url] = true;
      items.push({ title: title, url: url, excerpt: excerpt, meta: meta, kind: "item" });
      if (items.length >= 30) break;
    }
    finish({
      title: clean(document.title),
      summary: "",
      items: items,
      itemCount: items.length
    });
  }
  setTimeout(read, 2200);
})();
"#;

const ZHIHU_DETAIL_SCRIPT: &str = r#"
(function () {
  var finished = false;
  var comments = [];
  function clean(value) {
    return (value || "").replace(/\s+/g, " ").trim();
  }
  function absolute(href) {
    try {
      var parsed = new URL(href, location.href);
      if (parsed.protocol !== "https:") return "";
      return parsed.href;
    } catch (_) {
      return "";
    }
  }
  function loginWall() {
    var path = location.pathname || "";
    if (/signin|signup|login/i.test(path)) return true;
    var sample = document.body ? document.body.innerText : "";
    return /扫码登录|登录\/注册|请先登录|密码登录/.test(sample);
  }
  function finish(payload) {
    if (finished) return;
    finished = true;
    try {
      payload.url = location.href;
      payload.path = location.pathname;
      payload.loginWall = loginWall();
      window.location.href = "pigo-collect://result?data=" +
        encodeURIComponent(JSON.stringify(payload));
    } catch (_) {}
  }
  function answerRows() {
    return Array.from(document.querySelectorAll(
      ".AnswerItem, [class*='AnswerItem'], [class*='AnswerCard'], .List-item"
    ));
  }
  function commentButtons(row) {
    return Array.from(row.querySelectorAll("button, [role='button']")).filter(function (node) {
      var label = clean(node.textContent);
      return /条评论/.test(label) && label.length <= 12;
    });
  }
  function readComments(row, next) {
    var buttons = commentButtons(row);
    if (!buttons.length) {
      next();
      return;
    }
    var button = buttons[0];
    try { button.click(); } catch (_) {}
    setTimeout(function () {
      var panel = row.querySelector(
        ".Comments-container, [class*='CommentList'], [class*='Comments']"
      );
      var rows = panel ? Array.from(panel.querySelectorAll(
        "[class*='CommentItem'], .CommentContent, [class*='comment-item']"
      )) : [];
      for (var i = 0; i < rows.length && i < 8; i++) {
        var comment = rows[i];
        var authorNode = comment.querySelector(
          "[class*='AuthorInfo-name'], .CommentAuthor, [class*='author']"
        );
        var bodyNode = comment.querySelector(".RichText, [class*='content'], p");
        var body = clean(bodyNode ? bodyNode.textContent : comment.textContent);
        if (!body) continue;
        comments.push({
          kind: "comment",
          title: clean(authorNode ? authorNode.textContent : "") || "评论",
          url: "",
          excerpt: body.slice(0, 400),
          meta: ""
        });
      }
      try { button.click(); } catch (_) {}
      next();
    }, 900);
  }
  function collectComments(rows, index, next) {
    if (index >= rows.length) {
      next();
      return;
    }
    readComments(rows[index], function () {
      collectComments(rows, index + 1, next);
    });
  }
  function finishPage() {
    var titleNode = document.querySelector(
      "h1.QuestionHeader-title, h1[class*='QuestionTitle'], h1"
    );
    var summaryNode = document.querySelector(
      ".QuestionRichText, [class*='QuestionRichText'], .QuestionHeader-detail"
    );
    var rows = answerRows();
    var items = [];
    for (var i = 0; i < rows.length && i < 10; i++) {
      var row = rows[i];
      var authorNode = row.querySelector(
        "[class*='AuthorInfo-name'], .UserLink, [class*='author']"
      );
      var bodyNode = row.querySelector(
        ".RichText, [class*='RichText'], [class*='content']"
      );
      var voteNode = row.querySelector("[class*='VoteButton'], [class*='vote']");
      var timeNode = row.querySelector("[class*='ContentItem-time'], [class*='time']");
      var body = clean(bodyNode ? bodyNode.textContent : "");
      if (!body) continue;
      var permalink = row.querySelector("a[href*='/answer/']");
      items.push({
        kind: "answer",
        title: clean(authorNode ? authorNode.textContent : "") || "回答",
        url: absolute(permalink ? permalink.getAttribute("href") : "") || location.href,
        excerpt: body.slice(0, 600),
        meta: [clean(voteNode ? voteNode.textContent : ""), clean(timeNode ? timeNode.textContent : "")]
          .filter(Boolean).join(" · ")
      });
    }
    finish({
      title: clean(titleNode ? titleNode.textContent : document.title),
      summary: clean(summaryNode ? summaryNode.textContent : "").slice(0, 600),
      items: items.concat(comments),
      itemCount: items.length + comments.length
    });
  }
  function start() {
    if (loginWall()) {
      finish({ title: clean(document.title), summary: "", items: [], itemCount: 0 });
      return;
    }
    var rounds = 0;
    function scrollStep() {
      window.scrollTo(0, document.body ? document.body.scrollHeight : 0);
      rounds += 1;
      if (rounds < 3) {
        setTimeout(scrollStep, 900);
        return;
      }
      var rows = answerRows().filter(function (row) {
        return commentButtons(row).length > 0;
      }).slice(0, 3);
      collectComments(rows, 0, finishPage);
    }
    scrollStep();
  }
  setTimeout(start, 1200);
})();
"#;

/// The Channels creator console renders inside a wujie micro-frontend, so the
/// content list lives in `wujie-app.shadowRoot`, not the light DOM. This script
/// traverses open shadow roots and reads `.post-list-body`. Rows are read
/// generically because no published-video account was available to pin their
/// exact class names.
const CHANNELS_LIST_SCRIPT: &str = r#"
(function () {
  var finished = false;
  function clean(value) {
    return (value || "").replace(/\s+/g, " ").trim();
  }
  function loginWall() {
    var path = location.pathname || "";
    if (/login/i.test(path)) return true;
    return !!document.querySelector(".qrcode-area, .qrcode-wrap");
  }
  function finish(payload) {
    if (finished) return;
    finished = true;
    try {
      payload.url = location.href;
      payload.path = location.pathname;
      payload.loginWall = loginWall();
      window.location.href = "pigo-collect://result?data=" +
        encodeURIComponent(JSON.stringify(payload));
    } catch (_) {}
  }
  function shadowRoots() {
    var roots = [document];
    for (var index = 0; index < roots.length && roots.length < 20; index++) {
      var nodes = roots[index].querySelectorAll("*");
      for (var i = 0; i < nodes.length; i++) {
        if (nodes[i].shadowRoot) roots.push(nodes[i].shadowRoot);
      }
    }
    return roots;
  }
  function deepQuery(selector) {
    var roots = shadowRoots();
    for (var i = 0; i < roots.length; i++) {
      var found = roots[i].querySelector(selector);
      if (found) return found;
    }
    return null;
  }
  function read(body) {
    var items = [];
    var seen = {};
    var rows = body ? Array.from(body.children) : [];
    for (var i = 0; i < rows.length && items.length < 30; i++) {
      var row = rows[i];
      var text = clean(row.innerText);
      if (!text) continue;
      var titleNode = row.querySelector(
        "[class*='title'], [class*='name'], h3, h4, strong"
      );
      var title = clean(titleNode ? titleNode.textContent : "") ||
        text.split("\n")[0].slice(0, 120);
      if (!title) continue;
      var key = title + "|" + text.slice(0, 80);
      if (seen[key]) continue;
      seen[key] = true;
      var timeNode = row.querySelector("[class*='time'], [class*='date']");
      var link = row.querySelector("a[href^='https://']");
      items.push({
        title: title,
        url: link ? link.href : "",
        excerpt: text.slice(0, 300),
        meta: clean(timeNode ? timeNode.textContent : ""),
        kind: "item"
      });
    }
    var empty = deepQuery(".none-post");
    var summary = "";
    if (items.length === 0 && empty) summary = "该账号还没有发表过视频";
    finish({
      title: "视频号已发表视频",
      summary: summary,
      items: items,
      itemCount: items.length
    });
  }
  var attempts = 0;
  function attempt() {
    if (loginWall()) {
      finish({ title: clean(document.title), summary: "", items: [], itemCount: 0 });
      return;
    }
    var body = deepQuery(".post-list-body");
    attempts += 1;
    if (!body && attempts < 8) {
      setTimeout(attempt, 800);
      return;
    }
    if (!body) {
      finish({ title: clean(document.title), summary: "没有找到视频列表容器", items: [], itemCount: 0 });
      return;
    }
    read(body);
  }
  setTimeout(attempt, 1500);
})();
"#;

/// The list-page template shared by the configured channels. `__CONFIG__` is replaced
/// by a JSON object with `links`, `title`, `card`, `excerpt`, `meta`, `max`
/// and `wait`.
const LIST_TEMPLATE: &str = r#"
(function () {
  var CFG = __CONFIG__;
  var finished = false;
  function clean(value) {
    return (value || "").replace(/\s+/g, " ").trim();
  }
  function absolute(href) {
    try {
      var parsed = new URL(href, location.href);
      if (parsed.protocol !== "https:") return "";
      return parsed.href;
    } catch (_) {
      return "";
    }
  }
  function loginWall() {
    var path = location.pathname || "";
    if (/signin|signup|login/i.test(path)) return true;
    var sample = document.body ? document.body.innerText : "";
    return /扫码登录|登录\/注册|请先登录|密码登录/.test(sample);
  }
  function finish(payload) {
    if (finished) return;
    finished = true;
    try {
      payload.url = location.href;
      payload.path = location.pathname;
      payload.loginWall = loginWall();
      window.location.href = "pigo-collect://result?data=" +
        encodeURIComponent(JSON.stringify(payload));
    } catch (_) {}
  }
  function firstText(scope, selectors) {
    if (!scope || !selectors) return "";
    for (var i = 0; i < selectors.length; i++) {
      var node = scope.querySelector(selectors[i]);
      if (!node) continue;
      var text = clean(node.textContent);
      if (text) return text;
    }
    return "";
  }
  function anchors() {
    var out = [];
    for (var i = 0; i < CFG.links.length; i++) {
      out = out.concat(Array.from(document.querySelectorAll(CFG.links[i])));
    }
    return out;
  }
  function read() {
    var rows = anchors();
    var seen = {};
    var items = [];
    for (var i = 0; i < rows.length; i++) {
      var anchor = rows[i];
      var url = absolute(anchor.getAttribute("href") || "");
      if (!url || seen[url]) continue;
      var title = firstText(anchor, CFG.title) || clean(anchor.textContent);
      if (!title) continue;
      var card = (CFG.card && anchor.closest(CFG.card.join(","))) || anchor.parentElement;
      var excerpt = "";
      var meta = "";
      if (card && card !== anchor) {
        excerpt = firstText(card, CFG.excerpt).slice(0, 240);
        meta = firstText(card, CFG.meta).slice(0, 80);
      }
      seen[url] = true;
      items.push({ title: title, url: url, excerpt: excerpt, meta: meta, kind: "item" });
      if (items.length >= CFG.max) break;
    }
    finish({
      title: clean(document.title),
      summary: "",
      items: items,
      itemCount: items.length
    });
  }
  setTimeout(read, CFG.wait);
})();
"#;

/// The detail-page template shared by the configured channels. Optional `post` and
/// `comments` specs read the rendered DOM; no endpoint is called.
const DETAIL_TEMPLATE: &str = r#"
(function () {
  var CFG = __CONFIG__;
  var finished = false;
  function clean(value) {
    return (value || "").replace(/\s+/g, " ").trim();
  }
  function absolute(href) {
    try {
      var parsed = new URL(href, location.href);
      if (parsed.protocol !== "https:") return "";
      return parsed.href;
    } catch (_) {
      return "";
    }
  }
  function loginWall() {
    var path = location.pathname || "";
    if (/signin|signup|login/i.test(path)) return true;
    var sample = document.body ? document.body.innerText : "";
    return /扫码登录|登录\/注册|请先登录|密码登录/.test(sample);
  }
  function finish(payload) {
    if (finished) return;
    finished = true;
    try {
      payload.url = location.href;
      payload.path = location.pathname;
      payload.loginWall = loginWall();
      window.location.href = "pigo-collect://result?data=" +
        encodeURIComponent(JSON.stringify(payload));
    } catch (_) {}
  }
  function firstText(scope, selectors) {
    if (!scope || !selectors) return "";
    for (var i = 0; i < selectors.length; i++) {
      var node = scope.querySelector(selectors[i]);
      if (!node) continue;
      var text = clean(node.textContent);
      if (text) return text;
    }
    return "";
  }
  function attr(scope, selector) {
    if (!scope || !selector) return "";
    var node = scope.querySelector(selector);
    return node ? absolute(node.getAttribute("href") || "") : "";
  }
  function readRow(row, spec, kind) {
    if (!row) return null;
    var bodyNode = spec.body && spec.body.length ? row.querySelector(spec.body.join(",")) : row;
    var body = clean(bodyNode ? bodyNode.textContent : "");
    if (!body) return null;
    var host = (spec.card && row.closest(spec.card)) || row;
    return {
      kind: kind,
      title: firstText(host, spec.author) || (kind === "comment" ? "评论" : "主题"),
      url: attr(host, spec.link) || "",
      excerpt: body.slice(0, kind === "comment" ? 400 : 600),
      meta: firstText(host, spec.meta)
    };
  }
  function read() {
    var title = firstText(document, CFG.title) || clean(document.title);
    var summary = firstText(document, CFG.summary).slice(0, 600);
    var items = [];
    if (CFG.post) {
      var post = readRow(document.querySelector(CFG.post.selector), CFG.post, "item");
      if (post) items.push(post);
    }
    if (CFG.comments) {
      var rows = Array.from(document.querySelectorAll(CFG.comments.selector));
      for (var i = 0; i < rows.length && i < CFG.comments.limit; i++) {
        var comment = readRow(rows[i], CFG.comments, "comment");
        if (comment) items.push(comment);
      }
    }
    finish({
      title: title,
      summary: summary,
      items: items,
      itemCount: items.length
    });
  }
  function start() {
    if (loginWall()) {
      finish({ title: clean(document.title), summary: "", items: [], itemCount: 0 });
      return;
    }
    var rounds = 0;
    function scrollStep() {
      window.scrollTo(0, document.body ? document.body.scrollHeight : 0);
      rounds += 1;
      if (rounds < CFG.scroll) {
        setTimeout(scrollStep, 900);
        return;
      }
      read();
    }
    scrollStep();
  }
  setTimeout(start, 1200);
})();
"#;

fn list_config(channel: &str) -> Option<Value> {
    match channel {
        "juejin" => Some(json!({
            "links": ["a[href*='/post/']"],
            "title": [".title", "h2", "h3", "[class*='title']"],
            "card": [".entry", "li", "article", "[class*='item']"],
            "excerpt": [".abstract", "[class*='abstract']", "[class*='description']", "[class*='summary']"],
            "meta": [".meta-container", "[class*='meta']"],
            "max": 30,
            "wait": 2200
        })),
        "bilibili" => Some(json!({
            "links": ["a[href*='/video/BV']"],
            "title": [".title", "h3", "[class*='title']"],
            "card": [".bili-video-card", ".video-list-item", "li", "[class*='card']"],
            "excerpt": [".bili-video-card__info--desc", "[class*='desc']", "[class*='summary']"],
            "meta": [".bili-video-card__stats", "[class*='stats']", "[class*='meta']"],
            "max": 30,
            "wait": 2400
        })),
        // The MP backend has no public search page: the owner pins the
        // "published content" list URL, and this reads that list's rows.
        "wechatMp" => Some(json!({
            "links": ["a[href*='/s/']", "a[href*='/s?']", "a[href*='mp.weixin.qq.com/s']"],
            "title": [".appmsg_title", "[class*='title']", "h3"],
            "card": [".appmsg_item", "tr", "li", "[class*='item']"],
            "excerpt": [".appmsg_digest", "[class*='digest']", "[class*='desc']"],
            "meta": [".appmsg_date", "[class*='date']", "[class*='meta']"],
            "max": 30,
            "wait": 2200
        })),
        _ => None,
    }
}

fn detail_config(channel: &str) -> Option<Value> {
    match channel {
        "juejin" => Some(json!({
            "title": [".article-title", "h1"],
            "summary": [".article-content", "article"],
            "scroll": 3,
            "post": null,
            "comments": {
                "selector": ".comment-item",
                "card": "",
                "author": [".comment-author", "[class*='author']"],
                "meta": [".comment-time", "time"],
                "link": "",
                "body": [".comment-content", "[class*='content']"],
                "limit": 20
            }
        })),
        // Bilibili renders comments inside open shadow roots, so this first
        // cut reads the video title and description only. Comments stay out
        // rather than reaching into a DOM shape that changes per release.
        "bilibili" => Some(json!({
            "title": [".video-title", "h1[title]", "h1"],
            "summary": [".desc-info-text", "#v_desc", "[class*='desc']"],
            "scroll": 2,
            "post": null,
            "comments": null
        })),
        // MP article comments load only after interaction and are often
        // absent; this reads the article body, which is the stable surface.
        "wechatMp" => Some(json!({
            "title": ["#activity-name", ".rich_media_title", "h1"],
            "summary": ["#js_content", ".rich_media_content"],
            "scroll": 2,
            "post": null,
            "comments": null
        })),
        _ => None,
    }
}

fn fill(template: &str, config: &Value) -> String {
    template.replace("__CONFIG__", &config.to_string())
}

/// The list extraction for one channel. Unknown channels keep the Zhihu
/// script, which is also the historical behaviour.
pub fn list_script(channel: &str) -> String {
    if channel == "wechatChannels" {
        return CHANNELS_LIST_SCRIPT.to_string();
    }
    list_config(channel)
        .map(|config| fill(LIST_TEMPLATE, &config))
        .unwrap_or_else(|| ZHIHU_LIST_SCRIPT.to_string())
}

/// The detail extraction for one channel.
pub fn detail_script(channel: &str) -> String {
    detail_config(channel)
        .map(|config| fill(DETAIL_TEMPLATE, &config))
        .unwrap_or_else(|| ZHIHU_DETAIL_SCRIPT.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_channel_gets_an_injected_script_without_network_calls() {
        for channel in ["zhihu", "juejin", "bilibili", "wechatMp", "wechatChannels"] {
            let list = list_script(channel);
            let detail = detail_script(channel);
            for script in [&list, &detail] {
                assert!(script.contains("querySelectorAll"), "{channel}");
                assert!(script.contains("pigo-collect://result"), "{channel}");
                assert!(!script.contains("fetch("), "{channel}");
                assert!(!script.contains("XMLHttpRequest"), "{channel}");
                assert!(!script.contains("__CONFIG__"), "{channel}");
            }
        }
    }

    #[test]
    fn juejin_uses_its_own_links_and_selectors() {
        let juejin = list_script("juejin");
        assert!(juejin.contains("a[href*='/post/']"));
        assert!(juejin.contains(".abstract"));
        assert!(detail_script("juejin").contains(".comment-item"));

        let bilibili = list_script("bilibili");
        assert!(bilibili.contains("a[href*='/video/BV']"));
        assert!(bilibili.contains(".bili-video-card"));
        let bilibili_detail = detail_script("bilibili");
        assert!(bilibili_detail.contains(".video-title"));
        assert!(bilibili_detail.contains(".desc-info-text"));

        let wechat = list_script("wechatMp");
        assert!(wechat.contains("a[href*='/s/']"));
        assert!(wechat.contains(".appmsg_title"));
        let wechat_detail = detail_script("wechatMp");
        assert!(wechat_detail.contains("#activity-name"));
        assert!(wechat_detail.contains("#js_content"));

        let channels = list_script("wechatChannels");
        assert!(channels.contains(".post-list-body"));
        assert!(channels.contains("shadowRoot"));
        assert!(channels.contains(".none-post"));
    }

    #[test]
    fn zhihu_keeps_its_click_to_read_comment_script() {
        let detail = detail_script("zhihu");
        assert!(detail.contains("条评论"));
        assert!(detail.contains(".click()"));
        assert!(!detail.contains("__CONFIG__"));
    }

    #[test]
    fn unknown_channels_fall_back_to_the_zhihu_script() {
        assert_eq!(list_script("unknown"), ZHIHU_LIST_SCRIPT);
        assert_eq!(detail_script("unknown"), ZHIHU_DETAIL_SCRIPT);
    }
}
