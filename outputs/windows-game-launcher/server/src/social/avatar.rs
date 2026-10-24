use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Upload {
    data: String,
}
fn normalize(data: &str) -> Result<Vec<u8>> {
    if data.len() > 700_000 {
        return Err(ApiError::bad("头像不能超过 500 KB"));
    }
    let raw = STANDARD
        .decode(data)
        .map_err(|_| ApiError::bad("头像编码无效"))?;
    if raw.len() > 512_000 {
        return Err(ApiError::bad("头像不能超过 500 KB"));
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(raw))
        .with_guessed_format()
        .map_err(|_| ApiError::bad("图片无效"))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|_| ApiError::bad("请使用尺寸不超过 4096 的 PNG、JPEG 或 WebP 图片"))?;
    let mut out = std::io::Cursor::new(Vec::new());
    decoded
        .thumbnail(256, 256)
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(|_| ApiError::bad("头像转换失败"))?;
    Ok(out.into_inner())
}
pub(super) async fn upload(
    State(app): State<App>,
    i: Identity,
    Json(v): Json<Upload>,
) -> Result<Json<Value>> {
    {
        let tx = limited(&app, i.user_id, "avatar", 5).await?;
        tx.commit().await?;
    }
    let bytes = tokio::task::spawn_blocking(move || normalize(&v.data))
        .await
        .map_err(|_| ApiError::bad("头像处理失败"))??;
    let storage = app
        .storage
        .as_ref()
        .ok_or_else(|| ApiError::bad("头像存储不可用"))?;
    let dir = tempfile::tempdir().map_err(|_| ApiError::bad("临时目录不可用"))?;
    let path = dir.path().join("avatar.png");
    tokio::fs::write(&path, &bytes)
        .await
        .map_err(|_| ApiError::bad("头像保存失败"))?;
    // A fixed separate key avoids accumulating abandoned objects or touching save retention.
    let mut tx = begin(&app, i.user_id).await?;
    tokio::time::timeout(
        std::time::Duration::from_secs(15),
        storage.publish(
            &format!("social/avatars/{}.png", i.user_id),
            &path,
            bytes.len() as u64,
        ),
    )
    .await
    .map_err(|_| ApiError::bad("头像上传超时"))??;
    sqlx::query("UPDATE social_profiles SET avatar='custom',avatar_bytes=$2,updated_at=now() WHERE user_id=$1").bind(i.user_id).bind(bytes.len() as i64).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}
pub(super) async fn read(
    State(app): State<App>,
    i: Identity,
    Path(owner): Path<Uuid>,
) -> Result<Json<Value>> {
    let mut tx = begin(&app, i.user_id).await?;
    allowed(&mut tx, i.user_id, owner).await?;
    let size: i64 = sqlx::query_scalar(
        "SELECT avatar_bytes FROM social_profiles WHERE user_id=$1 AND avatar='custom'",
    )
    .bind(owner)
    .fetch_optional(&mut *tx)
    .await?
    .unwrap_or(0);
    if size <= 0 || size > 512_000 {
        return Err(ApiError::bad("头像不存在"));
    }
    let storage = app
        .storage
        .as_ref()
        .ok_or_else(|| ApiError::bad("头像存储不可用"))?;
    let dir = tempfile::tempdir().map_err(|_| ApiError::bad("临时目录不可用"))?;
    let path = dir.path().join("avatar.png");
    tokio::time::timeout(
        std::time::Duration::from_secs(15),
        storage.fetch(&format!("social/avatars/{owner}.png"), &path, size as u64),
    )
    .await
    .map_err(|_| ApiError::bad("头像读取超时"))??;
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|_| ApiError::bad("读取头像失败"))?;
    tx.commit().await?;
    Ok(Json(
        json!({"data":format!("data:image/png;base64,{}",STANDARD.encode(bytes))}),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_large_or_invalid_images() {
        assert!(normalize(&"a".repeat(700001)).is_err());
        assert!(normalize("YWJj").is_err());
    }
    #[test]
    fn normalizes_image() {
        let mut b = std::io::Cursor::new(vec![]);
        image::DynamicImage::new_rgb8(512, 512)
            .write_to(&mut b, image::ImageFormat::Png)
            .unwrap();
        let out = normalize(&STANDARD.encode(b.into_inner())).unwrap();
        let im = image::load_from_memory(&out).unwrap();
        assert_eq!(im.width(), 256);
    }
}
