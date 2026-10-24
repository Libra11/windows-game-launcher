#![allow(unused_must_use)]
use super::*;
use axum::extract::Query;
use std::sync::Arc;
fn ok<T>(r: Result<T>) -> T {
    r.unwrap_or_else(|e| panic!("{} {}", e.0, e.1))
}
async fn user(pool: &sqlx::PgPool) -> Identity {
    let u = Uuid::new_v4();
    let d = Uuid::new_v4();
    sqlx::query("INSERT INTO users(id,email) VALUES($1,$2)")
        .bind(u)
        .bind(format!("social-test-{u}@example.invalid"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO devices(user_id,id,name) VALUES($1,$2,'social-test')")
        .bind(u)
        .bind(d)
        .execute(pool)
        .await
        .unwrap();
    Identity {
        user_id: u,
        device_id: d,
        session_id: Uuid::new_v4(),
    }
}
async fn beat(app: &App, i: Identity, games: Value, announce: bool) {
    ok(presence::heartbeat(
        State(app.clone()),
        i,
        Json(serde_json::from_value(json!({"games":games,"announce":announce})).unwrap()),
    )
    .await);
}
async fn events(app: &App, i: Identity, cursor: Option<i64>) -> Value {
    ok(presence::events(
        State(app.clone()),
        i,
        Query(serde_json::from_value(json!({"cursor":cursor})).unwrap()),
    )
    .await)
    .0
}
#[tokio::test]
async fn postgres_friends_privacy_presence_and_revocation() {
    let url = std::env::var("YOUJI_TEST_DATABASE_URL")
        .expect("run social integration tests with an isolated YOUJI_TEST_DATABASE_URL");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    crate::migrate(&pool).await.unwrap();
    let app = App {
        pool: pool.clone(),
        storage: None,
        http: reqwest::Client::new(),
        config: Arc::new(crate::config::Config {
            token_key: "test-only-long-enough-123456789012345".into(),
            mail: crate::mail::MailConfig::Http {
                url: "https://invalid.test".into(),
                token: "test".into(),
            },
            bind: "127.0.0.1:0".into(),
        }),
    };
    let a = user(&pool).await;
    let b = user(&pool).await;
    let stranger = user(&pool).await;
    let ma = ok(me(State(app.clone()), a).await).0;
    ok(me(State(app.clone()), b).await);
    ok(me(State(app.clone()), stranger).await);
    assert!(!ma.to_string().contains("example.invalid"));
    let page = || Query(serde_json::from_value(json!({"offset":0})).unwrap());
    assert!(space::space(State(app.clone()), b, Path(a.user_id), page())
        .await
        .is_err());
    ok(request(
        State(app.clone()),
        b,
        Json(Request {
            code: ma["friendCode"].as_str().unwrap().into(),
        }),
    )
    .await);
    ok(request(
        State(app.clone()),
        b,
        Json(Request {
            code: ma["friendCode"].as_str().unwrap().into(),
        }),
    )
    .await);
    assert!(relation(
        State(app.clone()),
        b,
        Path(a.user_id),
        Json(Action {
            action: "accept".into()
        })
    )
    .await
    .is_err());
    ok(relation(
        State(app.clone()),
        a,
        Path(b.user_id),
        Json(Action {
            action: "accept".into(),
        }),
    )
    .await);
    ok(space::space(State(app.clone()), b, Path(a.user_id), page()).await);
    assert!(
        space::space(State(app.clone()), stranger, Path(a.user_id), page())
            .await
            .is_err()
    );
    let c = events(&app, b, None).await["cursor"].as_i64().unwrap();
    let session = Uuid::new_v4();
    let game = json!([{"key":"steam:10","title":"测试游戏","sessionId":session}]);
    beat(&app, a, game.clone(), true).await;
    beat(&app, a, game.clone(), true).await;
    let e = events(&app, b, Some(c)).await;
    assert_eq!(e["events"].as_array().unwrap().len(), 1);
    let c = e["cursor"].as_i64().unwrap();
    // A burst larger than one page must not advance past unseen events.
    sqlx::query("INSERT INTO social_events(recipient,actor,game_key,game_title) SELECT $1,$2,'steam:10','burst' FROM generate_series(1,101)").bind(b.user_id).bind(a.user_id).execute(&pool).await.unwrap();
    let first=events(&app,b,Some(c)).await;
    assert_eq!(first["events"].as_array().unwrap().len(),100);
    let second=events(&app,b,first["cursor"].as_i64()).await;
    assert_eq!(second["events"].as_array().unwrap().len(),1);
    let c=second["cursor"].as_i64().unwrap();
    // Second device running the same game must not emit another user-level start.
    let mut a2 = a;
    a2.device_id = Uuid::new_v4();
    sqlx::query("INSERT INTO devices(user_id,id,name) VALUES($1,$2,'second')")
        .bind(a.user_id)
        .bind(a2.device_id)
        .execute(&pool)
        .await
        .unwrap();
    beat(
        &app,
        a2,
        json!([{"key":"steam:10","title":"测试游戏","sessionId":Uuid::new_v4()}]),
        true,
    )
    .await;
    assert!(events(&app, b, Some(c)).await["events"]
        .as_array()
        .unwrap()
        .is_empty());
    let mut p = Privacy::default();
    p.invisible = true;
    ok(save_me(
        State(app.clone()),
        a,
        Json(Edit {
            nickname: "玩家 A".into(),
            bio: "".into(),
            avatar: "builtin:leaf".into(),
            privacy: p.clone(),
        }),
    )
    .await);
    let mut tx = begin(&app, b.user_id).await.unwrap();
    assert_eq!(
        ok(presence::status(&mut tx, a.user_id).await)["online"],
        false
    );
    tx.commit().await.unwrap();
    p.invisible = false;
    p.hidden_games = vec!["steam:10".into()];
    ok(save_me(
        State(app.clone()),
        a,
        Json(Edit {
            nickname: "玩家 A".into(),
            bio: "".into(),
            avatar: "builtin:leaf".into(),
            privacy: p,
        }),
    )
    .await);
    let mut tx = begin(&app, b.user_id).await.unwrap();
    assert!(ok(presence::status(&mut tx, a.user_id).await)["games"]
        .as_array()
        .unwrap()
        .is_empty());
    tx.commit().await.unwrap();
    // Muting consumes no queued alert, rejection remains non-friend, and reconnect is silent.
    let code_b = ok(me(State(app.clone()), b).await).0["friendCode"]
        .as_str()
        .unwrap()
        .to_string();
    ok(request(State(app.clone()), stranger, Json(Request { code: code_b })).await);
    ok(relation(
        State(app.clone()),
        b,
        Path(stranger.user_id),
        Json(Action {
            action: "reject".into(),
        }),
    )
    .await);
    assert!(
        space::space(State(app.clone()), stranger, Path(b.user_id), page())
            .await
            .is_err()
    );
    ok(relation(
        State(app.clone()),
        b,
        Path(a.user_id),
        Json(Action {
            action: "mute".into(),
        }),
    )
    .await);
    beat(
        &app,
        a,
        json!([{"key":"steam:20","title":"Other","sessionId":Uuid::new_v4()}]),
        true,
    )
    .await;
    assert!(events(&app, b, None).await["events"]
        .as_array()
        .unwrap()
        .is_empty());
    ok(relation(
        State(app.clone()),
        b,
        Path(a.user_id),
        Json(Action {
            action: "unmute".into(),
        }),
    )
    .await);
    let mark = events(&app, b, None).await["cursor"].as_i64();
    beat(
        &app,
        a,
        json!([{"key":"steam:30","title":"Reconnect","sessionId":Uuid::new_v4()}]),
        false,
    )
    .await;
    assert!(events(&app, b, mark).await["events"]
        .as_array()
        .unwrap()
        .is_empty());
    // A malformed/unknown code still consumes the request rate budget.
    for _ in 0..11 {
        let _ = request(
            State(app.clone()),
            stranger,
            Json(Request {
                code: "invalid".into(),
            }),
        )
        .await;
    }
    let e = request(
        State(app.clone()),
        stranger,
        Json(Request {
            code: "invalid".into(),
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(e.0, axum::http::StatusCode::TOO_MANY_REQUESTS);
    // Authenticated friend events are re-filtered; deleting a relationship removes access.
    ok(relation(
        State(app.clone()),
        b,
        Path(a.user_id),
        Json(Action {
            action: "block".into(),
        }),
    )
    .await);
    assert!(space::space(State(app.clone()), a, Path(b.user_id), page())
        .await
        .is_err());
    assert!(space::space(State(app.clone()), b, Path(a.user_id), page())
        .await
        .is_err());
    assert!(events(&app, b, Some(c)).await["events"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(request(
        State(app.clone()),
        a,
        Json(Request {
            code: ok(me(State(app.clone()), b).await).0["friendCode"]
                .as_str()
                .unwrap()
                .into()
        })
    )
    .await
    .is_err());
    // Expired and revoked devices cannot keep a user online.
    sqlx::query(
        "UPDATE social_presence SET updated_at=now()-interval '91 seconds' WHERE user_id=$1",
    )
    .bind(a.user_id)
    .execute(&pool)
    .await
    .unwrap();
    let mut tx = begin(&app, b.user_id).await.unwrap();
    assert_eq!(
        ok(presence::status(&mut tx, a.user_id).await)["online"],
        false
    );
    tx.commit().await.unwrap();
}
#[test]
fn projection_hides_games_and_keeps_manual_and_official_distinct() {
    let data = vec![
        (
            "game".into(),
            "g".into(),
            json!({"source":"steam","sourceIdentity":"10","displayTitle":"Safe","metadataReference":"10","achievementReference":"10"}),
        ),
        (
            "historical_baseline".into(),
            "g".into(),
            json!({"seconds":60}),
        ),
        (
            "play_session".into(),
            "s".into(),
            json!({"gameId":"g","seconds":120,"startedAt":"2026-10-24T00:00:00Z"}),
        ),
        (
            "official_playtime".into(),
            "g".into(),
            json!({"seconds":500,"accountId":"PRIVATE"}),
        ),
        (
            "definition_cache".into(),
            "g".into(),
            json!({"namespace":"10","definitions":[{"apiName":"a","name":"A","description":"text","icon":"https://example.invalid/icon.png"}]}),
        ),
        (
            "manual_unlock".into(),
            "u".into(),
            json!({"gameId":"g","namespace":"10","apiName":"a","present":true}),
        ),
    ];
    let mut p = Privacy::default();
    let out = space::project(&data, &p, false);
    assert_eq!(out[0]["localSeconds"], 180);
    assert_eq!(out[0]["officialSeconds"], 500);
    assert_eq!(out[0]["achievements"][0]["source"], "manual");
    assert_eq!(out[0]["achievements"][0]["icon"], "https://example.invalid/icon.png");
    assert!(!json!(out).to_string().contains("PRIVATE"));
    p.playtime = false;
    p.achievements = false;
    let out = space::project(&data, &p, false);
    assert!(out[0]["localSeconds"].is_null());
    assert!(out[0]["achievements"].is_null());
    p.hidden_games.push("steam:10".into());
    assert!(space::project(&data, &p, false).is_empty());
    assert_eq!(space::project(&data, &p, true).len(), 1);
}
