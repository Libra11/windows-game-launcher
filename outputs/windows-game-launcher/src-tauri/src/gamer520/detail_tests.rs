use super::{detail, downloads};

#[test]
fn adjacent_text_urls_are_separate_and_script_strings_are_not_downloads() {
    let url = reqwest::Url::parse("https://520.gamers520.com/1.html").unwrap();
    let html = r#"<article id="post-1"><div class="entry-content"><p>https://pan.baidu.com/s/onehttps://pan.quark.cn/s/two</p><p><script>https://pan.baidu.com/s/hidden</script></p><p><a href="javascript:open('https://pan.baidu.com/s/script')">点我</a></p></div></article>"#;
    let data = serde_json::to_value(downloads::parse_page(html, &url).unwrap()).unwrap();
    assert_eq!(data["links"].as_array().unwrap().len(), 2);
    assert_eq!(data["links"][0]["url"], "https://pan.baidu.com/s/one");
    assert_eq!(data["links"][1]["url"], "https://pan.quark.cn/s/two");
}

#[test]
fn grime_plain_text_share_and_following_code_are_read_without_navigation_ads() {
    let url = reqwest::Url::parse("https://520.gamers520.com/17318.html").unwrap();
    let html = r#"<nav>https://pan.baidu.com/s/advert</nav><article id="post-17318"><div class="entry-content"><p>链接: https://pan.baidu.com/s/grime</p><p>提取码: vvxk</p><p><strong>Onedrive</strong><strong>盘获取=右下角 [立即获取]</strong></p><p><strong>解压密码:laoquzhang.com</strong></p><p>中文修改器[外站]:<a href="https://2468c.com/2905.html">点我</a></p></div></article>"#;
    let data = serde_json::to_value(downloads::parse_page(html, &url).unwrap()).unwrap();
    assert_eq!(data["links"].as_array().unwrap().len(), 1);
    assert_eq!(data["links"][0]["url"], "https://pan.baidu.com/s/grime");
    assert_eq!(data["links"][0]["code"], "vvxk");
    assert_eq!(data["error"], "");
    assert!(data["notes"][0].as_str().unwrap().contains("Onedrive"));
}

#[test]
fn text_links_handle_chinese_punctuation_and_deduplicate_anchor_text() {
    let url = reqwest::Url::parse("https://520.gamers520.com/1.html").unwrap();
    let html = r#"<article id="post-1"><div class="entry-content"><p>链接：https://pan.baidu.com/s/game?pwd=abcd。<a href="https://pan.baidu.com/s/game?pwd=abcd">https://pan.baidu.com/s/game?pwd=abcd</a></p><p>链接: https://pan.baidu.com.evil.example/s/bad</p></div></article>"#;
    let data = serde_json::to_value(downloads::parse_page(html, &url).unwrap()).unwrap();
    assert_eq!(data["links"].as_array().unwrap().len(), 1);
    assert_eq!(data["links"][0]["code"], "abcd");
}

#[tokio::test]
#[ignore = "显式复现 GRIME 密码 8915 后的纯文本网盘地址"]
async fn live_grime_plain_text_download() {
    let data = serde_json::to_value(
        detail::get_gamer520_detail("https://www.gamer520.com/21176.html".into())
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(data["downloads"]["error"], "");
    assert_eq!(data["downloads"]["filePassword"], "8915");
    let links = data["downloads"]["links"].as_array().unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0]["label"], "百度网盘");
    assert_eq!(
        links[0]["url"],
        "https://pan.baidu.com/s/1JCLVdaU1SPabtXRPbqw7qQ"
    );
    assert_eq!(links[0]["code"], "vvxk");
    println!("GRIME 正常密码流程后读取到百度网盘地址、提取码和解压密码，错误提示为空");
}

#[test]
fn original_page_fallback_works_before_and_after_download_resolution() {
    for value in [
        "https://www.gamer520.com/21602.html",
        "https://www.gamer520.com/go?post_id=21602",
        "https://520.gamers520.com/13352.html",
    ] {
        assert!(downloads::resource_target(value).is_some());
    }
    for value in [
        "https://www.gamer520.com/wp-login.php",
        "https://www.gamer520.com/go?post_id=oops",
        "https://www.gamer520.com/go?post_id=1&redirect=https://evil.example",
    ] {
        assert!(downloads::resource_target(value).is_none());
    }
}

#[test]
fn detail_keeps_text_and_images_without_scripts_and_rejects_wrong_identity() {
    let url = detail::article_url("https://www.gamer520.com/1.html").unwrap();
    let html = r#"<body class="postid-1"><article id="post-1"><h1 class="entry-title">测试游戏</h1><div class="entry-content"><p>游戏简介<script>alert(1)</script></p><h2>配置要求</h2><p><img src="https://images.example/test.jpg"><img src="javascript:alert(2)"></p></div></article></body>"#;
    let data = serde_json::to_value(detail::parse(html, &url).unwrap()).unwrap();
    assert_eq!(data["blocks"][0]["text"], "游戏简介");
    assert_eq!(data["blocks"].as_array().unwrap().len(), 3);
    assert!(detail::parse(&html.replace("postid-1", "postid-2"), &url).is_err());
    assert!(detail::article_url("https://www.gamer520.com/go?post_id=1").is_none());
}

#[test]
fn reads_qr_payloads_codes_and_direct_links_only_from_resource_content() {
    let url = reqwest::Url::parse("https://520.gamers520.com/1.html").unwrap();
    let html = r#"<nav><a href="https://pan.baidu.com/s/advert">广告</a></nav><article id="post-1"><div class="entry-content"><p>百度网盘扫码：</p><div class="wpkqcg_qrcode_wrapper"><input id="qr_content" value="https://pan.baidu.com/s/game"><img src=""></div><p>提取码:abcd</p><p><a href="https://pan.quark.cn/s/game">夸克</a><a href="https://pan.quark.cn/s/game">重复</a><a href="javascript:alert(1)">伪链接</a></p><p>解压密码：example</p></div></article>"#;
    let data = serde_json::to_value(downloads::parse_page(html, &url).unwrap()).unwrap();
    assert_eq!(data["links"].as_array().unwrap().len(), 2);
    assert_eq!(data["links"][0]["code"], "abcd");
    assert_eq!(data["archivePassword"], "example");
    for value in [
        "https://pan.baidu.com.evil.example/s/game",
        "https://pan.baidu.com/",
        "file:///tmp/a",
    ] {
        assert!(downloads::cloud_url(value).is_none());
    }
    let locked = r#"<article id="post-1"><div class="entry-content"><form class="post-password-form"></form></div></article>"#;
    assert!(downloads::parse_page(locked, &url).is_err());
}

#[tokio::test]
#[ignore = "显式联网验证详情、原站公开密码流程及网盘二维码链接"]
async fn live_detail_downloads() {
    let detail = detail::get_gamer520_detail("https://www.gamer520.com/21602.html".into())
        .await
        .unwrap();
    assert!(
        detail.downloads.error.is_empty(),
        "{}",
        detail.downloads.error
    );
    let data = serde_json::to_value(detail).unwrap();
    assert!(!data["blocks"].as_array().unwrap().is_empty());
    let links = data["downloads"]["links"].as_array().unwrap();
    assert!(links.len() >= 4);
    for label in ["百度网盘", "夸克网盘", "迅雷网盘", "UC 网盘"] {
        assert!(links.iter().any(|link| link["label"] == label));
    }
    println!(
        "实际详情正文 {} 段，网盘 {} 项，二维码源链接及密码信息读取成功",
        data["blocks"].as_array().unwrap().len(),
        links.len()
    );
}
