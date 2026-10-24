use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use sha2::{Digest, Sha256};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Upload { key: String, data: String }
#[derive(Deserialize)]
pub(super) struct Query { key: String }
fn object(owner: Uuid, key: &str) -> String {
    format!("social/covers/{owner}/{:x}.jpg", Sha256::digest(key.as_bytes()))
}
async fn game_exists(tx: &mut Tx<'_>, owner: Uuid, key: &str) -> Result<()> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sync_resources WHERE user_id=$1 AND kind='game' AND NOT deleted AND (payload->>'source')||':'|| (payload->>'sourceIdentity')=$2)")
        .bind(owner).bind(key).fetch_one(&mut **tx).await?;
    if !exists { return Err(ApiError::bad("请先同步此游戏资料")); }
    Ok(())
}
fn normalize(data: &str) -> Result<(Vec<u8>, String)> {
    if data.len()>11_200_000 { return Err(ApiError::bad("封面不能超过 8 MB")); }
    let raw=STANDARD.decode(data).map_err(|_|ApiError::bad("封面编码无效"))?;
    if raw.len()>8*1024*1024 {return Err(ApiError::bad("封面不能超过 8 MB"));}
    let hash=format!("{:x}",Sha256::digest(&raw));
    let mut reader=image::ImageReader::new(std::io::Cursor::new(raw)).with_guessed_format().map_err(|_|ApiError::bad("封面格式无效"))?;
    let mut limits=image::Limits::default();limits.max_image_width=Some(8192);limits.max_image_height=Some(8192);limits.max_alloc=Some(128*1024*1024);reader.limits(limits);
    let im=reader.decode().map_err(|_|ApiError::bad("封面图片无效或过大"))?.thumbnail(600,900).to_rgb8();
    let mut bytes=vec![];
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes,85).encode_image(&im).map_err(|_|ApiError::bad("封面转换失败"))?;
    if bytes.len()>1_000_000 {return Err(ApiError::bad("封面转换后过大"));}
    Ok((bytes,hash))
}
pub(super) async fn manifest(State(app):State<App>, i:Identity)->Result<Json<Value>> {
    let rows=sqlx::query("SELECT game_key,source_hash FROM social_covers WHERE user_id=$1").bind(i.user_id).fetch_all(&app.pool).await?;
    let keys:Vec<String>=sqlx::query_scalar("SELECT (payload->>'source')||':'||(payload->>'sourceIdentity') FROM sync_resources WHERE user_id=$1 AND kind='game' AND NOT deleted").bind(i.user_id).fetch_all(&app.pool).await?;
    Ok(Json(json!({"gameKeys":keys,"covers":rows.iter().map(|r|json!({"key":r.get::<String,_>("game_key"),"hash":r.get::<String,_>("source_hash")})).collect::<Vec<_>>()})))
}
pub(super) async fn upload(State(app):State<App>, i:Identity, Json(v):Json<Upload>)->Result<Json<Value>> {
    {let mut tx=limited(&app,i.user_id,"cover",120).await?;game_exists(&mut tx,i.user_id,&v.key).await?;tx.commit().await?;}
    let (bytes,hash)=tokio::task::spawn_blocking(move||normalize(&v.data)).await.map_err(|_|ApiError::bad("封面处理失败"))??;
    let mut tx=begin(&app,i.user_id).await?;
    game_exists(&mut tx,i.user_id,&v.key).await?;
    let old:Option<String>=sqlx::query_scalar("SELECT source_hash FROM social_covers WHERE user_id=$1 AND game_key=$2").bind(i.user_id).bind(&v.key).fetch_optional(&mut *tx).await?;
    if old.as_deref()!=Some(&hash) {
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM social_covers WHERE user_id=$1").bind(i.user_id).fetch_one(&mut *tx).await?;
        if old.is_none()&&count>=5000 {return Err(ApiError::bad("空间封面数量达到上限"));}
        let storage=app.storage.as_ref().ok_or_else(||ApiError::bad("封面存储不可用"))?;
        let dir=tempfile::tempdir().map_err(|_|ApiError::bad("临时目录不可用"))?;let path=dir.path().join("cover.jpg");
        tokio::fs::write(&path,&bytes).await.map_err(|_|ApiError::bad("封面保存失败"))?;
        tokio::time::timeout(std::time::Duration::from_secs(20),storage.publish(&object(i.user_id,&v.key),&path,bytes.len() as u64)).await.map_err(|_|ApiError::bad("封面上传超时"))??;
        sqlx::query("INSERT INTO social_covers VALUES($1,$2,$3,$4) ON CONFLICT(user_id,game_key) DO UPDATE SET source_hash=excluded.source_hash,bytes=excluded.bytes").bind(i.user_id).bind(&v.key).bind(&hash).bind(bytes.len() as i64).execute(&mut *tx).await?;
    }
    tx.commit().await?;Ok(Json(json!({"hash":hash})))
}
pub(super) async fn read(State(app):State<App>,i:Identity,Path(owner):Path<Uuid>,axum::extract::Query(q):axum::extract::Query<Query>)->Result<Json<Value>> {
    let mut tx=begin(&app,i.user_id).await?;allowed(&mut tx,i.user_id,owner).await?;game_exists(&mut tx,owner,&q.key).await?;
    let p=privacy(&mut tx,owner).await?;
    if i.user_id!=owner&&(!p.library||p.hidden_games.contains(&q.key)){return Err(ApiError(axum::http::StatusCode::NOT_FOUND,"封面不可访问".into()));}
    let size:Option<i64>=sqlx::query_scalar("SELECT bytes FROM social_covers WHERE user_id=$1 AND game_key=$2").bind(owner).bind(&q.key).fetch_optional(&mut *tx).await?;
    let size=size.ok_or_else(||ApiError::bad("封面尚未上传"))?;
    let storage=app.storage.as_ref().ok_or_else(||ApiError::bad("封面存储不可用"))?;
    let dir=tempfile::tempdir().map_err(|_|ApiError::bad("临时目录不可用"))?;let path=dir.path().join("cover.jpg");
    tokio::time::timeout(std::time::Duration::from_secs(20),storage.fetch(&object(owner,&q.key),&path,size as u64)).await.map_err(|_|ApiError::bad("封面读取超时"))??;
    let bytes=tokio::fs::read(path).await.map_err(|_|ApiError::bad("封面读取失败"))?;
    tx.commit().await?;Ok(Json(json!({"data":format!("data:image/jpeg;base64,{}",STANDARD.encode(bytes))})))
}
#[cfg(test)]mod tests {
 use super::*;
 #[test]fn image_is_normalized(){let mut b=std::io::Cursor::new(vec![]);image::DynamicImage::new_rgb8(1200,1800).write_to(&mut b,image::ImageFormat::Png).unwrap();let (out,hash)=normalize(&STANDARD.encode(b.into_inner())).unwrap();let im=image::load_from_memory(&out).unwrap();assert_eq!((im.width(),im.height()),(600,900));assert_eq!(hash.len(),64);assert!(normalize("YWJj").is_err());}
}
