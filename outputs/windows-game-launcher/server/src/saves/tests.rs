use super::*;
#[test]
fn only_direct_current_parent_advances_selection() {
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    assert!(advances(None, 0, 0, &[]));
    assert!(advances(Some(a), 2, 2, &[a]));
    assert!(!advances(Some(a), 2, 1, &[a]));
    assert!(!advances(Some(a), 2, 2, &[b]));
    assert!(!advances(Some(a), 2, 2, &[]));
    assert!(!advances(Some(a), 2, 2, &[a, b]));
}

fn ok<T>(result: Result<T>) -> T {
    result.unwrap_or_else(|e| panic!("{}: {}", e.0, e.1))
}
#[derive(Clone, Default)]
struct MockS3 {
    objects: Arc<tokio::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>>,
    public: Arc<std::sync::atomic::AtomicBool>,
    delete_failed: Arc<std::sync::atomic::AtomicBool>,
}
async fn mock_s3(
    State(state): State<MockS3>,
    request: axum::extract::Request,
) -> axum::response::Response {
    use axum::{
        http::{Method, StatusCode},
        response::IntoResponse,
    };
    let path = request.uri().path().to_owned();
    let query: std::collections::HashMap<_, _> =
        url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes())
            .into_owned()
            .collect();
    if query.contains_key("publicAccessBlock") {
        return "<PublicAccessBlockConfiguration><BlockPublicAccess>true</BlockPublicAccess></PublicAccessBlockConfiguration>".into_response();
    }
    if query.contains_key("policyStatus") {
        return format!(
            "<PolicyStatus><IsPublic>{}</IsPublic></PolicyStatus>",
            state.public.load(std::sync::atomic::Ordering::SeqCst)
        )
        .into_response();
    }
    if query.contains_key("acl") {
        return "<AccessControlPolicy><Owner><ID>owner</ID></Owner><AccessControlList><Grant>private</Grant></AccessControlList></AccessControlPolicy>".into_response();
    }
    if query.contains_key("encryption") {
        return "<ServerSideEncryptionRule><ApplyServerSideEncryptionByDefault><SSEAlgorithm>AES256</SSEAlgorithm></ApplyServerSideEncryptionByDefault></ServerSideEncryptionRule>".into_response();
    }
    match *request.method() {
        Method::PUT => {
            let bytes = axum::body::to_bytes(request.into_body(), 1024 * 1024)
                .await
                .unwrap();
            state.objects.lock().await.insert(path, bytes.to_vec());
            StatusCode::OK.into_response()
        }
        Method::GET => state
            .objects
            .lock()
            .await
            .get(&path)
            .cloned()
            .map(IntoResponse::into_response)
            .unwrap_or(StatusCode::NOT_FOUND.into_response()),
        Method::DELETE => {
            if state
                .delete_failed
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                return StatusCode::SERVICE_UNAVAILABLE.into_response();
            }
            state.objects.lock().await.remove(&path);
            StatusCode::NO_CONTENT.into_response()
        }
        _ => StatusCode::METHOD_NOT_ALLOWED.into_response(),
    }
}
async fn identity(pool: &sqlx::PgPool) -> Identity {
    let user = Uuid::new_v4();
    let device = Uuid::new_v4();
    sqlx::query("INSERT INTO users(id,email) VALUES($1,$2)")
        .bind(user)
        .bind(format!("{user}@test.invalid"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO devices(user_id,id,name) VALUES($1,$2,'测试设备')")
        .bind(user)
        .bind(device)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO user_sync_state(user_id,initialized) VALUES($1,true)")
        .bind(user)
        .execute(pool)
        .await
        .unwrap();
    Identity {
        user_id: user,
        device_id: device,
        session_id: Uuid::new_v4(),
    }
}
#[tokio::test]
async fn postgres_private_upload_quota_forks_and_gc() {
    use axum::extract::Path;
    use youji_cloud_protocol::saves::UploadRequest;
    let Ok(url) = std::env::var("YOUJI_TEST_DATABASE_URL") else {
        eprintln!("未设置 YOUJI_TEST_DATABASE_URL，跳过隔离 Postgres 测试");
        return;
    };
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    let state = MockS3::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let router = Router::new().fallback(mock_s3).with_state(state.clone());
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let storage = Arc::new(crate::storage::Storage::test(endpoint.clone(), 16384));
    let app = App {
        pool: pool.clone(),
        storage: Some(storage.clone()),
        http: reqwest::Client::new(),
        config: Arc::new(crate::config::Config {
            token_key: "test-only-long-enough-key-123456789012345".into(),
            mail: crate::mail::MailConfig::Http {
                url: "https://invalid.test".into(),
                token: "test".into(),
            },
            bind: "127.0.0.1:0".into(),
        }),
    };
    let quota_app = App {
        storage: Some(Arc::new(crate::storage::Storage::test(endpoint, 2048))),
        ..app.clone()
    };
    let owner = identity(&pool).await;
    let other = identity(&pool).await;
    let game = Uuid::new_v4();
    sqlx::query("INSERT INTO sync_resources(user_id,kind,id,revision,payload,device_id) VALUES($1,'game',$2,1,'{}',$3)")
        .bind(owner.user_id).bind(game.to_string()).bind(owner.device_id).execute(&pool).await.unwrap();
    assert!(create_profile(
        State(app.clone()),
        other,
        Json(CreateProfile {
            game_id: game,
            name: "越权".into()
        })
    )
    .await
    .is_err());
    let p = ok(create_profile(
        State(app.clone()),
        owner,
        Json(CreateProfile {
            game_id: game,
            name: "主存档".into(),
        }),
    )
    .await)
    .0;
    let profile_id = Uuid::parse_str(p["id"].as_str().unwrap()).unwrap();
    let temp = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let root = temp.path().join("files");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("save.dat"), b"hello save").unwrap();
    let archive = temp.path().join("save.zip");
    let manifest = youji_save_archive::create(&root, &archive).unwrap();
    let bytes = std::fs::read(&archive).unwrap();
    let base = UploadRequest {
        snapshot_id: Uuid::new_v4(),
        profile_id,
        parent_ids: vec![],
        base_revision: 0,
        manifest_hash: youji_cloud_protocol::digest(&manifest).unwrap(),
        archive_hash: youji_save_archive::file_hash(&archive).unwrap(),
        bytes: bytes.len() as u64,
    };
    // 两个 1536 字节预留竞争 2048 配额，必须仅一个成功。
    let mut q1 = base.clone();
    q1.snapshot_id = Uuid::new_v4();
    q1.bytes = 1536;
    let mut q2 = q1.clone();
    q2.snapshot_id = Uuid::new_v4();
    let (a, b) = tokio::join!(
        create_upload(State(quota_app.clone()), owner, Json(q1)),
        create_upload(State(quota_app.clone()), owner, Json(q2))
    );
    assert_ne!(a.is_ok(), b.is_ok());
    sqlx::query("UPDATE save_uploads SET expires_at=now()-interval '1 second' WHERE user_id=$1")
        .bind(owner.user_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(reserved(&pool, owner.user_id).await, 1536);
    let mut again = base.clone();
    again.snapshot_id = Uuid::new_v4();
    again.bytes = 1536;
    assert!(
        create_upload(State(quota_app.clone()), owner, Json(again))
            .await
            .is_err(),
        "过期但未删除的对象仍占配额"
    );
    sqlx::query("UPDATE save_object_gc SET not_before=now()-interval '1 second' WHERE user_id=$1")
        .bind(owner.user_id)
        .execute(&pool)
        .await
        .unwrap();
    ok(cleanup(&app).await);
    assert_eq!(reserved(&pool, owner.user_id).await, 0);
    // 暂存 1200 字节后，2048 配额只允许一次 600 字节发布尝试。
    let mut qa = base.clone();
    qa.snapshot_id = Uuid::new_v4();
    qa.bytes = 600;
    let mut qb = qa.clone();
    qb.snapshot_id = Uuid::new_v4();
    for request in [qa.clone(), qb.clone()] {
        let upload = ok(create_upload(State(quota_app.clone()), owner, Json(request)).await).0;
        let _ = ok(complete_upload(
            State(quota_app.clone()),
            owner,
            Path(Uuid::parse_str(upload["uploadId"].as_str().unwrap()).unwrap()),
        )
        .await);
    }
    let (a, b) = tokio::join!(jobs::claim(&quota_app), jobs::claim(&quota_app));
    let (a, b) = (ok(a), ok(b));
    assert_ne!(a.is_some(), b.is_some());
    let claimed = a.or(b).unwrap();
    assert!(ok(jobs::claim(&quota_app).await).is_none());
    assert_eq!(reserved(&pool, owner.user_id).await, 1800);
    let request: UploadRequest = serde_json::from_value(claimed.request.clone()).unwrap();
    ok(jobs::fail(&quota_app, &claimed, ApiError::bad("invalid archive")).await);
    let retry = ok(create_upload(State(quota_app.clone()), owner, Json(request)).await).0;
    let _ = ok(complete_upload(
        State(quota_app.clone()),
        owner,
        Path(Uuid::parse_str(retry["uploadId"].as_str().unwrap()).unwrap()),
    )
    .await);
    assert!(
        ok(jobs::claim(&quota_app).await).is_none(),
        "重置 attempts 不得绕过历史尝试容量"
    );
    assert_eq!(reserved(&pool, owner.user_id).await, 1800);
    sqlx::query("UPDATE save_uploads SET expires_at=now()-interval '1 second' WHERE user_id=$1")
        .bind(owner.user_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE save_object_gc SET not_before=now()-interval '1 second' WHERE user_id=$1")
        .bind(owner.user_id)
        .execute(&pool)
        .await
        .unwrap();
    state
        .delete_failed
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(cleanup(&app).await.is_err());
    assert_eq!(
        reserved(&pool, owner.user_id).await,
        1800,
        "OSS 删除失败不能释放容量"
    );
    state
        .delete_failed
        .store(false, std::sync::atomic::Ordering::SeqCst);
    ok(cleanup(&app).await);
    assert_eq!(reserved(&pool, owner.user_id).await, 0);
    let up = ok(create_upload(State(app.clone()), owner, Json(base.clone())).await).0;
    let expired_id = Uuid::parse_str(up["uploadId"].as_str().unwrap()).unwrap();
    sqlx::query(
        "UPDATE save_uploads SET expires_at=now()-interval '1 second' WHERE user_id=$1 AND id=$2",
    )
    .bind(owner.user_id)
    .bind(expired_id)
    .execute(&pool)
    .await
    .unwrap();
    let up = ok(create_upload(State(app.clone()), owner, Json(base.clone())).await).0;
    assert_ne!(up["uploadId"], json!(expired_id));
    assert!(complete_upload(State(app.clone()), owner, Path(expired_id))
        .await
        .is_err());
    let upload_id = Uuid::parse_str(up["uploadId"].as_str().unwrap()).unwrap();
    assert!(complete_upload(State(app.clone()), other, Path(upload_id))
        .await
        .is_err());
    // 未验证的上传不出现在快照列表。
    assert_eq!(
        ok(list_snapshots(State(app.clone()), owner, Path(profile_id)).await).0["snapshots"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    let put = |url: String, data: Vec<u8>| async move {
        reqwest::Client::new()
            .put(url)
            .header("x-oss-server-side-encryption", "AES256")
            .body(data)
            .send()
            .await
            .unwrap()
    };
    put(up["uploadUrl"].as_str().unwrap().into(), b"wrong".to_vec()).await;
    let queued = ok(complete_upload(State(app.clone()), owner, Path(upload_id)).await);
    assert_eq!(queued.0, axum::http::StatusCode::ACCEPTED);
    assert_eq!(queued.1 .0["status"], "queued");
    assert_eq!(
        ok(complete_upload(State(app.clone()), owner, Path(upload_id)).await)
            .1
             .0,
        queued.1 .0
    );
    assert!(upload_status(State(app.clone()), other, Path(upload_id))
        .await
        .is_err());
    let (first, second) = tokio::join!(jobs::claim(&app), jobs::claim(&app));
    let (first, second) = (ok(first), ok(second));
    assert_ne!(first.is_some(), second.is_some());
    let job = first.or(second).unwrap();
    let error = verification::verify(
        &app,
        &job,
        temp.path(),
        Arc::new(tokio::sync::Semaphore::new(1)),
    )
    .await
    .err()
    .expect("损坏归档必须失败");
    let before_failure = reserved(&pool, owner.user_id).await;
    ok(jobs::fail(&app, &job, error).await);
    assert_eq!(
        reserved(&pool, owner.user_id).await,
        before_failure,
        "失败不能释放尝试对象预留"
    );
    assert_eq!(
        ok(upload_status(State(app.clone()), owner, Path(upload_id)).await).0["status"],
        "failed"
    );
    assert_eq!(
        ok(list_snapshots(State(app.clone()), owner, Path(profile_id)).await).0["snapshots"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    // 用户重试重新上传；领取后模拟进程退出，租约到期由另一个 worker 接续。
    let up = ok(create_upload(State(app.clone()), owner, Json(base.clone())).await).0;
    put(up["uploadUrl"].as_str().unwrap().into(), bytes.clone()).await;
    let _ = ok(complete_upload(State(app.clone()), owner, Path(upload_id)).await);
    let old = ok(jobs::claim(&app).await).unwrap();
    assert_eq!(
        ok(create_upload(State(app.clone()), owner, Json(base.clone())).await).0["status"],
        "validating"
    );
    sqlx::query(
        "UPDATE save_uploads SET lease_until=now()-interval '1 second' WHERE user_id=$1 AND id=$2",
    )
    .bind(owner.user_id)
    .bind(upload_id)
    .execute(&pool)
    .await
    .unwrap();
    let before_reclaim = reserved(&pool, owner.user_id).await;
    let current = ok(jobs::claim(&app).await).unwrap();
    assert_eq!(
        reserved(&pool, owner.user_id).await,
        before_reclaim + base.bytes as i64
    );
    assert_ne!(old.token, current.token);
    assert_ne!(old.object_key, current.object_key);
    assert!(jobs::renew(&app, &old).await.is_err());
    assert!(verification::verify(
        &app,
        &old,
        temp.path(),
        Arc::new(tokio::sync::Semaphore::new(1))
    )
    .await
    .is_err());
    ok(jobs::renew(&app, &current).await);
    let before_publish = reserved(&pool, owner.user_id).await;
    ok(verification::verify(
        &app,
        &current,
        temp.path(),
        Arc::new(tokio::sync::Semaphore::new(1)),
    )
    .await);
    assert_eq!(
        reserved(&pool, owner.user_id).await,
        before_publish,
        "正式发布只转账，不重复计费"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM save_object_gc WHERE object_key=$1")
            .bind(&current.object_key)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    let completed = ok(complete_upload(State(app.clone()), owner, Path(upload_id)).await);
    assert_eq!(completed.0, axum::http::StatusCode::OK);
    assert_eq!(completed.1 .0["status"], "completed");
    assert_eq!(
        ok(complete_upload(State(app.clone()), owner, Path(upload_id)).await)
            .1
             .0,
        completed.1 .0
    );
    // 校验后再次写暂存地址，不影响已发布的不可变对象。
    put(up["uploadUrl"].as_str().unwrap().into(), b"tamper".to_vec()).await;
    let url = ok(download(State(app.clone()), owner, Path(base.snapshot_id)).await).0["url"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        reqwest::get(url)
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap()
            .as_ref(),
        bytes
    );
    assert!(download(State(app.clone()), other, Path(base.snapshot_id))
        .await
        .is_err());
    state
        .public
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(download(State(app.clone()), owner, Path(base.snapshot_id))
        .await
        .is_err());
    state
        .public
        .store(false, std::sync::atomic::Ordering::SeqCst);
    // 从旧/空父版本发布，保留为分叉，不推进当前版本。
    let mut fork = base.clone();
    fork.snapshot_id = Uuid::new_v4();
    let up = ok(create_upload(State(app.clone()), owner, Json(fork.clone())).await).0;
    put(up["uploadUrl"].as_str().unwrap().into(), bytes.clone()).await;
    let _ = ok(complete_upload(
        State(app.clone()),
        owner,
        Path(Uuid::parse_str(up["uploadId"].as_str().unwrap()).unwrap()),
    )
    .await);
    let job = ok(jobs::claim(&app).await).unwrap();
    let objects_before = state.objects.lock().await.len();
    let reserved_before = reserved(&pool, owner.user_id).await;
    ok(verification::verify(
        &app,
        &job,
        temp.path(),
        Arc::new(tokio::sync::Semaphore::new(1)),
    )
    .await);
    assert_eq!(
        state.objects.lock().await.len(),
        objects_before,
        "查重命中不能发布无用副本"
    );
    assert_eq!(
        reserved(&pool, owner.user_id).await,
        reserved_before - base.bytes as i64,
        "未写入的尝试键释放空预留"
    );
    let listing = ok(list_snapshots(State(app.clone()), owner, Path(profile_id)).await).0;
    assert_eq!(
        listing["profile"]["selectedSnapshotId"],
        json!(base.snapshot_id)
    );
    assert_eq!(listing["snapshots"].as_array().unwrap().len(), 2);
    assert!(
        delete_snapshot(State(app.clone()), owner, Path(base.snapshot_id))
            .await
            .is_err()
    );
    // 分叉内容相同，用户内对象只计费一次。
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM save_objects WHERE user_id=$1")
        .bind(owner.user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    assert!(history::resolve(
        State(app.clone()),
        owner,
        Path(profile_id),
        Json(history::Resolve {
            snapshot_id: fork.snapshot_id,
            base_revision: 0,
            parent_ids: vec![base.snapshot_id, fork.snapshot_id]
        })
    )
    .await
    .is_err());
    let resolved = ok(history::resolve(
        State(app.clone()),
        owner,
        Path(profile_id),
        Json(history::Resolve {
            snapshot_id: fork.snapshot_id,
            base_revision: 1,
            parent_ids: vec![base.snapshot_id, fork.snapshot_id],
        }),
    )
    .await)
    .0;
    assert_eq!(resolved["parentIds"].as_array().unwrap().len(), 2);
    assert!(
        delete_snapshot(State(app.clone()), owner, Path(fork.snapshot_id))
            .await
            .is_err()
    );
    // 不能把另一档案的快照作为父版本。
    let second_profile = ok(create_profile(
        State(app.clone()),
        owner,
        Json(CreateProfile {
            game_id: game,
            name: "其他档案".into(),
        }),
    )
    .await)
    .0;
    let mut invalid_parent = base.clone();
    invalid_parent.snapshot_id = Uuid::new_v4();
    invalid_parent.profile_id = Uuid::parse_str(second_profile["id"].as_str().unwrap()).unwrap();
    invalid_parent.parent_ids = vec![base.snapshot_id];
    assert!(
        create_upload(State(app.clone()), owner, Json(invalid_parent))
            .await
            .is_err()
    );
    // 非当前、无子引用的分叉可删除，不删除共用的已发布对象。
    let mut leaf = base.clone();
    leaf.snapshot_id = Uuid::new_v4();
    let up = ok(create_upload(State(app.clone()), owner, Json(leaf.clone())).await).0;
    put(up["uploadUrl"].as_str().unwrap().into(), bytes.clone()).await;
    let _ = ok(complete_upload(
        State(app.clone()),
        owner,
        Path(Uuid::parse_str(up["uploadId"].as_str().unwrap()).unwrap()),
    )
    .await);
    let job = ok(jobs::claim(&app).await).unwrap();
    ok(verification::verify(
        &app,
        &job,
        temp.path(),
        Arc::new(tokio::sync::Semaphore::new(1)),
    )
    .await);
    let _ = ok(delete_snapshot(State(app.clone()), owner, Path(leaf.snapshot_id)).await);
    let _ = ok(delete_snapshot(State(app.clone()), owner, Path(leaf.snapshot_id)).await);
    assert!(download(State(app.clone()), owner, Path(leaf.snapshot_id))
        .await
        .is_err());
    // 临时存储错误最多自动尝试三次；重启中断同样计入上限。
    let mut retry = base.clone();
    retry.snapshot_id = Uuid::new_v4();
    let pending = ok(create_upload(State(app.clone()), owner, Json(retry.clone())).await).0;
    let retry_id = Uuid::parse_str(pending["uploadId"].as_str().unwrap()).unwrap();
    let _ = ok(complete_upload(State(app.clone()), owner, Path(retry_id)).await);
    for attempt in 1..=2 {
        let job = ok(jobs::claim(&app).await).unwrap();
        ok(jobs::fail(&app, &job, internal()).await);
        let status = ok(upload_status(State(app.clone()), owner, Path(retry_id)).await).0;
        assert_eq!(status["status"], "queued");
        assert_eq!(status["attempts"], attempt);
        sqlx::query("UPDATE save_uploads SET next_attempt_at=now() WHERE user_id=$1 AND id=$2")
            .bind(owner.user_id)
            .bind(retry_id)
            .execute(&pool)
            .await
            .unwrap();
    }
    let last = ok(jobs::claim(&app).await).unwrap();
    sqlx::query(
        "UPDATE save_uploads SET lease_until=now()-interval '1 second' WHERE user_id=$1 AND id=$2",
    )
    .bind(owner.user_id)
    .bind(retry_id)
    .execute(&pool)
    .await
    .unwrap();
    assert!(ok(jobs::claim(&app).await).is_none());
    assert_eq!(
        ok(upload_status(State(app.clone()), owner, Path(retry_id)).await).0["status"],
        "failed"
    );
    // 失去租约的旧任务不能再把失败状态变回排队。
    ok(jobs::fail(&app, &last, internal()).await);
    assert_eq!(
        ok(upload_status(State(app.clone()), owner, Path(retry_id)).await).0["status"],
        "failed"
    );
    std::fs::write(root.join("save.dat"), b"different contents").unwrap();
    let second_archive = temp.path().join("second.zip");
    let second_manifest = youji_save_archive::create(&root, &second_archive).unwrap();
    let second_bytes = std::fs::read(&second_archive).unwrap();
    let unique = UploadRequest {
        snapshot_id: Uuid::new_v4(),
        manifest_hash: youji_cloud_protocol::digest(&second_manifest).unwrap(),
        archive_hash: youji_save_archive::file_hash(&second_archive).unwrap(),
        bytes: second_bytes.len() as u64,
        ..base.clone()
    };
    let up = ok(create_upload(State(app.clone()), owner, Json(unique.clone())).await).0;
    put(up["uploadUrl"].as_str().unwrap().into(), second_bytes).await;
    let _ = ok(complete_upload(
        State(app.clone()),
        owner,
        Path(Uuid::parse_str(up["uploadId"].as_str().unwrap()).unwrap()),
    )
    .await);
    let job = ok(jobs::claim(&app).await).unwrap();
    ok(verification::verify(
        &app,
        &job,
        temp.path(),
        Arc::new(tokio::sync::Semaphore::new(1)),
    )
    .await);
    let before_delete = reserved(&pool, owner.user_id).await;
    let _ = ok(delete_snapshot(State(app.clone()), owner, Path(unique.snapshot_id)).await);
    assert_eq!(
        reserved(&pool, owner.user_id).await,
        before_delete,
        "删除索引不释放实际对象容量"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT bytes FROM save_object_gc WHERE object_key=$1")
            .bind(&job.object_key)
            .fetch_one(&pool)
            .await
            .unwrap(),
        unique.bytes as i64
    );
    // 过期暂存 GC 不删除仍被快照引用的最终对象。
    sqlx::query("UPDATE save_uploads SET expires_at=now()-interval '1 second' WHERE user_id=$1")
        .bind(owner.user_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE save_object_gc SET not_before=now()-interval '1 second' WHERE object_key LIKE $1 OR object_key LIKE $2")
        .bind(format!("staging/{}/%",owner.user_id)).bind(format!("objects/{}/%",owner.user_id)).execute(&pool).await.unwrap();
    ok(cleanup(&app).await);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM save_objects WHERE user_id=$1")
            .bind(owner.user_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(reserved(&pool, owner.user_id).await, base.bytes as i64);
    task.abort();
}

async fn reserved(pool: &sqlx::PgPool, user: Uuid) -> i64 {
    sqlx::query_scalar("SELECT COALESCE((SELECT sum(bytes) FROM save_objects WHERE user_id=$1),0)::bigint + COALESCE((SELECT sum(bytes) FROM save_object_gc WHERE user_id=$1),0)::bigint")
        .bind(user).fetch_one(pool).await.unwrap()
}

#[tokio::test]
async fn postgres_retention_keeps_graph_pins_pending_and_accounts_for_delayed_gc() {
    let Ok(url) = std::env::var("YOUJI_TEST_DATABASE_URL") else {
        eprintln!("需要隔离 Postgres");
        return;
    };
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    let who = identity(&pool).await;
    let profile = Uuid::new_v4();
    sqlx::query("INSERT INTO save_profiles(user_id,id,game_id,name,auto_retention) VALUES($1,$2,$3,'retention',true)").bind(who.user_id).bind(profile).bind(Uuid::new_v4()).execute(&pool).await.unwrap();
    let mut ids = Vec::new();
    for i in 0..15 {
        let id = Uuid::new_v4();
        let object = Uuid::new_v4();
        sqlx::query("INSERT INTO save_objects VALUES($1,$2,$3,$4,$4,100,'{}',true)")
            .bind(who.user_id)
            .bind(object)
            .bind(format!("objects/test/{object}"))
            .bind("a".repeat(64))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO save_snapshots(user_id,id,profile_id,device_id,object_id,created_at,pinned) VALUES($1,$2,$3,$4,$5,date_trunc('day',now())+interval '1 second'*$6,$7)").bind(who.user_id).bind(id).bind(profile).bind(who.device_id).bind(object).bind(i as f64).bind(i==0).execute(&pool).await.unwrap();
        if let Some(parent) = ids.last() {
            sqlx::query("INSERT INTO save_snapshot_parents VALUES($1,$2,$3,$4)")
                .bind(who.user_id)
                .bind(profile)
                .bind(id)
                .bind(parent)
                .execute(&pool)
                .await
                .unwrap();
        }
        ids.push(id);
    }
    sqlx::query("UPDATE save_profiles SET selected_snapshot_id=$3 WHERE user_id=$1 AND id=$2")
        .bind(who.user_id)
        .bind(profile)
        .bind(ids[14])
        .execute(&pool)
        .await
        .unwrap();
    let upload = Uuid::new_v4();
    sqlx::query("INSERT INTO save_uploads(user_id,id,profile_id,device_id,snapshot_id,request,bytes,staging_key,object_key) VALUES($1,$2,$3,$4,$5,$6,100,$7,$8)").bind(who.user_id).bind(upload).bind(profile).bind(who.device_id).bind(Uuid::new_v4()).bind(json!({"parentIds":[ids[1]]})).bind(format!("staging/{upload}")).bind(format!("objects/{upload}")).execute(&pool).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    assert_eq!(ok(policy::prune_locked(&mut tx, who.user_id).await), 3);
    assert_eq!(ok(uploads::quota_used(&mut tx, who.user_id).await), 1500);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM save_snapshots WHERE user_id=$1")
            .bind(who.user_id)
            .fetch_one(&mut *tx)
            .await
            .unwrap(),
        15
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM save_snapshot_parents WHERE user_id=$1")
            .bind(who.user_id)
            .fetch_one(&mut *tx)
            .await
            .unwrap(),
        14
    );
    assert_eq!(ok(policy::prune_locked(&mut tx, who.user_id).await), 0);
    let removed = ok(snapshot(&mut tx, who.user_id, ids[2]).await);
    assert_eq!(removed["available"], false);
    tx.commit().await.unwrap();
    let app = App {
        pool: pool.clone(),
        storage: Some(Arc::new(crate::storage::Storage::test(
            "http://127.0.0.1:1".into(),
            16384,
        ))),
        http: reqwest::Client::new(),
        config: Arc::new(crate::config::Config {
            token_key: "test-only-long-enough-key-123456789012345".into(),
            mail: crate::mail::MailConfig::Http {
                url: "https://invalid.test".into(),
                token: "test".into(),
            },
            bind: "127.0.0.1:0".into(),
        }),
    };
    let error = download(State(app), who, axum::extract::Path(ids[2]))
        .await
        .err()
        .expect("已清理版本不得签发下载地址");
    assert!(error.1.contains("已清理"));
}
