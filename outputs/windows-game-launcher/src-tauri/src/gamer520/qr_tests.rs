use super::{detail, downloads};
use reqwest::Url;

fn fixture(content: &str) -> String {
    format!(r#"<article id="post-39577"><div class="entry-content">{content}</div></article>"#)
}

fn parse(content: &str) -> serde_json::Value {
    let url = Url::parse("https://520.gamers520.com/39577.html").unwrap();
    serde_json::to_value(downloads::parse_page(&fixture(content), &url).unwrap()).unwrap()
}

#[test]
fn qr_image_payloads_and_direct_link_produce_three_cloud_entries() {
    let html = r#"<div class="bdp-grid"><div class="bdp-card bdp-baidu"><div class="bdp-card-title">百度网盘扫码</div><div class="bdp-qrcode-box"><img src="https://api.qrserver.com/v1/create-qr-code/?size=150x150&#038;data=https%3A%2F%2Fpan.baidu.com%2Fs%2Fgame%3Fpwd%3Dryjv"></div><div class="bdp-pwd-box">提取码: <strong>ryjv</strong></div></div><div class="bdp-card bdp-quark"><div class="bdp-qrcode-box"><img src="https://api.qrserver.com/v1/create-qr-code/?size=150x150&amp;data=https%3A%2F%2Fpan.quark.cn%2Fs%2Fgame"></div></div><div class="bdp-card bdp-xunlei"><a href="https://pan.xunlei.com/s/game?pwd=xqfs">极速云盘下载</a></div></div>"#;
    let data = parse(html);
    let links = data["links"].as_array().unwrap();
    assert_eq!(links.len(), 3);
    assert_eq!(links[0]["label"], "百度网盘");
    assert_eq!(links[0]["code"], "ryjv");
    assert_eq!(links[1]["label"], "夸克网盘");
    assert_eq!(links[1]["url"], "https://pan.quark.cn/s/game");
    assert_eq!(links[2]["label"], "迅雷网盘");
    assert_eq!(data["error"], "");
}

#[test]
fn lazy_qr_image_reads_card_code_and_deduplicates_visible_share_link() {
    let html = r#"<div class="bdp-card"><div class="bdp-qrcode-box"><img src="data:image/gif;base64,abc" data-src="https://api.qrserver.com/v1/create-qr-code/?data=https%3A%2F%2Fpan.baidu.com%2Fs%2Fgame"></div><div class="bdp-pwd-box">提取码：<strong>abcd</strong></div><a href="https://pan.baidu.com/s/game">打开</a></div>"#;
    let data = parse(html);
    assert_eq!(data["links"].as_array().unwrap().len(), 1);
    assert_eq!(data["links"][0]["code"], "abcd");
}

#[test]
fn untrusted_qr_hosts_ambiguous_data_and_invalid_targets_are_rejected() {
    for source in [
        "https://evil.example/v1/create-qr-code/?data=https%3A%2F%2Fpan.baidu.com%2Fs%2Fgame",
        "https://api.qrserver.com/v1/create-qr-code/?data=javascript%3Aalert%281%29",
        "https://api.qrserver.com/v1/create-qr-code/?data=https%3A%2F%2Fpan.baidu.com.evil.example%2Fs%2Fgame",
        "https://api.qrserver.com/v1/create-qr-code/?data=https%3A%2F%2Fuser%3Asecret%40pan.baidu.com%2Fs%2Fgame",
        "https://api.qrserver.com/v1/create-qr-code/?data=https%3A%2F%2Fpan.baidu.com%2Fs%2Fa&amp;data=https%3A%2F%2Fpan.baidu.com%2Fs%2Fb",
    ] {
        let data=parse(&format!(r#"<div class="bdp-qrcode-box"><img src="{source}"></div>"#));
        assert!(data["links"].as_array().unwrap().is_empty());
    }
    // 原站文章外的导航图片由 parse_page 的正文范围排除。
    let outside = fixture("")
        + r#"<div class="bdp-qrcode-box"><img src="https://api.qrserver.com/v1/create-qr-code/?data=https%3A%2F%2Fpan.baidu.com%2Fs%2Fadvert"></div>"#;
    let url = Url::parse("https://520.gamers520.com/39577.html").unwrap();
    assert!(
        serde_json::to_value(downloads::parse_page(&outside, &url).unwrap()).unwrap()["links"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
#[ignore = "显式联网验证 007 的百度／夸克二维码和迅雷分享入口"]
async fn live_007_reads_all_three_clouds() {
    let data = serde_json::to_value(
        detail::get_gamer520_detail("https://www.gamer520.com/114287.html".into())
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        data["downloads"]["sourceUrl"],
        "https://520.gamers520.com/39577.html"
    );
    assert_eq!(data["downloads"]["error"], "");
    let links = data["downloads"]["links"].as_array().unwrap();
    assert_eq!(links.len(), 3);
    for label in ["百度网盘", "夸克网盘", "迅雷网盘"] {
        assert!(links.iter().any(|link| link["label"] == label));
    }
    let baidu = links
        .iter()
        .find(|link| link["label"] == "百度网盘")
        .unwrap();
    assert_eq!(baidu["code"], "ryjv");
    println!("007 资源页识别到百度、夸克、迅雷共 3 项；百度提取码正确；二维码源站没有被请求");
}
