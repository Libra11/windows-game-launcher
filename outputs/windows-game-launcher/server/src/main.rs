use youji_cloud_server::{config::Config, App};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(10)
        .connect(&std::env::var("DATABASE_URL")?)
        .await?;
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("migrate") {
        youji_cloud_server::migrate(&pool).await?;
        println!("数据库结构已初始化");
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("invite") {
        let email = youji_cloud_server::auth::email(
            args.get(2).ok_or("用法：youji-cloud-server invite 邮箱")?,
        )
        .map_err(|e| e.1)?;
        sqlx::query("INSERT INTO invites(email) VALUES($1) ON CONFLICT DO NOTHING")
            .bind(email)
            .execute(&pool)
            .await?;
        println!("邀请已保存");
        return Ok(());
    }
    let config = Config::from_env()?;
    let bind = config.bind.clone();
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let storage = youji_cloud_server::storage::Storage::from_env()
        .map_err(|e| e.1)?
        .map(std::sync::Arc::new);
    let app = App {
        pool,
        config: std::sync::Arc::new(config),
        http,
        storage,
    };
    if args.get(1).map(String::as_str) == Some("worker") {
        youji_cloud_server::saves::run_workers(app)
            .await
            .map_err(|e| e.1)?;
        return Ok(());
    }
    if args.len() > 1 {
        return Err("未知命令：支持 migrate、invite、worker".into());
    }
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    axum::serve(listener, youji_cloud_server::router(app)).await?;
    Ok(())
}
