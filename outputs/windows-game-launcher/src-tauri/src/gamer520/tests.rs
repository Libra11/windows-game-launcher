use super::*;

fn fixture(content: &str, pagination: &str) -> String {
    format!(
        r#"<body class="search search-results"><h1 class="term-title">搜索：空洞骑士</h1><main class="site-main"><div class="posts-wrapper">{content}</div><div class="numeric-pagination">{pagination}</div></main></body>"#
    )
}

fn row(id: &str, href: &str) -> String {
    format!(
        r#"<article id="post-{id}" class="category-pcplay"><div class="entry-media"><img src="data:image/gif;base64,abc" data-src="https://images.example/cover.jpg"></div><div class="meta-category"><a>PC PLAY</a></div><h2 class="entry-title"><a href="{href}">空洞骑士 &amp; DLC</a></h2><time datetime="2026-10-12T00:00:00+08:00"></time></article>"#
    )
}

#[test]
fn search_url_encodes_keywords_and_validates_input() {
    let url = search_url("A & 中文", 2).unwrap();
    assert_eq!(url.path(), "/page/2");
    assert_eq!(
        url.query_pairs().collect::<Vec<_>>(),
        vec![("s".into(), "A & 中文".into())]
    );
    for (query, page) in [("", 1), ("a\nb", 1), ("game", 0), ("game", 101)] {
        assert!(search_url(query, page).is_err());
    }
    assert!(search_url(&"中".repeat(101), 1).is_err());
}

#[test]
fn parses_results_lazy_images_categories_and_next_page() {
    let html = fixture(
        &row("21602", "/21602.html"),
        r#"<a class="next page-numbers" href="/page/2?s=空洞骑士">下一页</a>"#,
    );
    let results = parser::parse(
        &html,
        "空洞骑士",
        1,
        search_url("空洞骑士", 1).unwrap().as_str(),
    )
    .unwrap();
    assert_eq!(results.items.len(), 1);
    let item = &results.items[0];
    assert_eq!(item.title, "空洞骑士 & DLC");
    assert_eq!(item.image, "https://images.example/cover.jpg");
    assert_eq!(item.url, "https://www.gamer520.com/21602.html");
    assert!(item.is_pc);
    assert_eq!(item.categories, ["PC PLAY"]);
    assert!(results.has_next);
}

#[test]
fn rejects_challenges_wrong_search_and_broken_layout_instead_of_empty_success() {
    for html in [
        "<html>Just a moment</html>".to_owned(),
        fixture("", ""),
        fixture(&row("1", "/1.html"), "").replace("搜索：空洞骑士", "搜索：其他游戏"),
    ] {
        assert!(parser::parse(&html, "空洞骑士", 1, ORIGIN).is_err());
    }
    let empty = r#"<body class="search search-no-results"><h1 class="term-title">搜索：空洞骑士</h1></body>"#;
    assert!(parser::parse(empty, "空洞骑士", 1, ORIGIN)
        .unwrap()
        .items
        .is_empty());
}

#[test]
fn excludes_external_links_duplicates_recommendations_and_wrong_next_page() {
    let rows = [
        row("1", "/1.html"),
        row("1", "/1.html"),
        row("2", "https://evil.example/2.html"),
        row("3", "/4.html"),
    ]
    .join("");
    let html = fixture(&rows, r#"<a class="next" href="/page/2?s=其他">下一页</a>"#)
        + &row("99", "/99.html");
    let results = parser::parse(&html, "空洞骑士", 1, ORIGIN).unwrap();
    assert_eq!(results.items.len(), 1);
    assert!(!results.has_next);
    for url in [
        "https://evil.example/",
        "javascript:alert(1)",
        "http://www.gamer520.com/",
        "https://user@www.gamer520.com/",
        "https://www.gamer520.com:8443/",
    ] {
        assert!(site_url(url).is_none(), "{url}");
    }
}

#[tokio::test]
#[ignore = "显式联网验证搜索、空结果和分页"]
async fn live_search_with_launcher_network_client() {
    for (query, page, empty) in [
        ("空洞骑士", 1, false),
        ("星露谷物语", 1, false),
        ("游迹不存在的游戏abcdef987654", 1, true),
        ("马里奥", 1, false),
        ("马里奥", 2, false),
    ] {
        let result = search_gamer520(query.into(), page).await.unwrap();
        assert_eq!(result.items.is_empty(), empty);
        if query == "马里奥" && page == 1 {
            assert!(result.has_next);
        }
        println!(
            "搜索={query} 页={page} 结果={} PC={} 下一页={}",
            result.items.len(),
            result.items.iter().filter(|item| item.is_pc).count(),
            result.has_next
        );
    }
}
