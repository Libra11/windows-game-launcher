use serde_json::Value;

pub(crate) struct Family {
    pub id: Option<String>,
    pub name: String,
}

pub(crate) fn number(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| value.as_str()?.parse().ok())
}

async fn request(method: &str, token: &str, fields: &[(&str, &str)]) -> Result<Value, String> {
    let response = crate::network::client(crate::network::Service::Steam)?
        .get(format!("https://api.steampowered.com/IFamilyGroupsService/{method}/v1/"))
        .timeout(std::time::Duration::from_secs(if method == "GetSharedLibraryApps" { 60 } else { 20 }))
        .query(&[("access_token", token)])
        .query(fields).send().await.map_err(|error| {
            // 请求含登录凭证，不能输出原始 URL 或 reqwest 错误。
            if error.is_timeout() { "Steam 家庭库连接超时，请检查网络和代理" }
            else { "无法连接 Steam 家庭库，请检查网络和代理" }.to_owned()
        })?;
    let status = response.status();
    if matches!(status.as_u16(), 401 | 403) {
        return Err("Steam 登录授权已失效或无家庭库读取权限，请重新登录".into());
    }
    if status.as_u16() == 429 { return Err("Steam 请求过于频繁，请稍后重试".into()); }
    if !status.is_success() { return Err("Steam 家庭库服务暂时不可用".into()); }
    if let Some(result) = response.headers().get("x-eresult").and_then(|value| value.to_str().ok()) {
        if result != "1" { return Err("Steam 未授权此次家庭库查询，请重新登录后重试".into()); }
    }
    let data: Value = response.json().await.map_err(|_| "Steam 家庭库响应格式无效")?;
    data.get("response").filter(|value| value.is_object()).cloned()
        .ok_or_else(|| "Steam 未返回有效的家庭库资料".into())
}

pub(crate) async fn family(token: &str, steam_id: &str) -> Result<Family, String> {
    let data = request("GetFamilyGroupForUser", token, &[
        ("steamid", steam_id), ("include_family_group_response", "true"),
    ]).await?;
    if data.get("is_not_member_of_any_group").and_then(Value::as_bool) == Some(true) {
        return Ok(Family { id: None, name: String::new() });
    }
    let id = data.get("family_groupid").and_then(number).filter(|id| *id > 0)
        .ok_or("Steam 未返回家庭组身份，请确认已加入 Steam 家庭")?.to_string();
    let group = data.get("family_group").filter(|group| group.is_object())
        .ok_or("Steam 未返回完整的家庭组资料")?;
    let members = group.get("members").and_then(Value::as_array)
        .ok_or("Steam 家庭组成员列表无效")?;
    if !members.iter().any(|member| member.get("steamid").and_then(number).map(|id| id.to_string()).as_deref() == Some(steam_id)) {
        return Err("Steam 返回的家庭组与当前登录账号不一致".into());
    }
    Ok(Family { id: Some(id), name: group.get("name").and_then(Value::as_str).unwrap_or("Steam 家庭").into() })
}

pub(crate) async fn library(token: &str, steam_id: &str, family_id: &str) -> Result<Value, String> {
    let mut data = request("GetSharedLibraryApps", token, &[
        ("family_groupid", family_id), ("steamid", steam_id),
        ("include_own", "true"), ("include_excluded", "true"),
        ("include_non_games", "false"), ("language", "schinese"),
    ]).await?;
    if data.get("owner_steamid").and_then(number).map(|id| id.to_string()).as_deref() != Some(steam_id) {
        return Err("Steam 返回的共享库不属于当前登录账号".into());
    }
    // protobuf 的空 repeated 字段可能被省略；仅在服务成功且账号验证后视为空库。
    if data.get("apps").is_none() { data["apps"] = serde_json::json!([]); }
    Ok(data)
}
