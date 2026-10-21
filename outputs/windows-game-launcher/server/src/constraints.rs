use crate::{Result,auth::Identity,sync::resource};
use sqlx::{PgConnection,Row};
use serde_json::Value;
use youji_cloud_protocol::*;

// 在用户同步锁内执行，确保唯一性、引用与业务判断对应同一版本。
pub async fn check(conn:&mut PgConnection,identity:Identity,op:&Operation,payload:&Value,importing:bool)->Result<std::result::Result<(),String>> {
    if op.action==Action::Delete {return Ok(Ok(()));}
    let kind=op.resource_type;
    if matches!(kind,ResourceType::HistoricalBaseline) && !importing {return Ok(Err("历史基线只能通过导入建立".into()));}
    if kind==ResourceType::PlaySession && payload["historical"]==true && !importing {return Ok(Err("历史会话只能通过导入建立".into()));}
    if kind==ResourceType::Game {
        let duplicate:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sync_resources WHERE user_id=$1 AND kind='game' AND id<>$2 AND payload->>'source'=$3 AND payload->>'sourceIdentity'=$4)").bind(identity.user_id).bind(&op.resource_id).bind(payload["source"].as_str()).bind(payload["sourceIdentity"].as_str()).fetch_one(&mut *conn).await?;
        if duplicate{return Ok(Err("该平台游戏已存在，请刷新身份映射".into()));}
    }
    if matches!(kind,ResourceType::Tag|ResourceType::Collection) {
        let wanted=payload["name"].as_str().unwrap_or("").to_lowercase();
        let rows=sqlx::query("SELECT id,payload FROM sync_resources WHERE user_id=$1 AND kind=$2 AND NOT deleted").bind(identity.user_id).bind(kind.key()).fetch_all(&mut *conn).await?;
        if rows.iter().any(|r|r.get::<String,_>("id")!=op.resource_id && r.get::<Value,_>("payload")["name"].as_str().is_some_and(|n|n.to_lowercase()==wanted)) {return Ok(Err("已存在同名分类，请选择已有分类或重命名".into()));}
    }
    let game=match kind {
        ResourceType::Title|ResourceType::MetadataReference|ResourceType::AchievementReference|ResourceType::Archive|ResourceType::Favorite =>Some(op.resource_id.as_str()),
        ResourceType::TagMember|ResourceType::CollectionMember|ResourceType::AutomaticUnlock|ResourceType::ManualUnlock|ResourceType::OfficialSnapshot|ResourceType::PlaySession|ResourceType::HistoricalBaseline|ResourceType::UnlockEvent|ResourceType::DefinitionCache|ResourceType::OfficialPlaytime=>payload["gameId"].as_str(),
        _=>None,
    };
    if let Some(id)=game {
        let g=resource(conn,identity.user_id,ResourceType::Game,id).await?;
        let Some(g)=g.filter(|g|!g.deleted) else{return Ok(Err("关联游戏不属于当前账号或不存在".into()));};
        if matches!(kind,ResourceType::AutomaticUnlock|ResourceType::ManualUnlock|ResourceType::OfficialSnapshot|ResourceType::DefinitionCache|ResourceType::UnlockEvent) {
            let reference=resource(conn,identity.user_id,ResourceType::AchievementReference,id).await?.map(|r|r.payload["value"].as_str().unwrap_or("").to_owned()).unwrap_or_else(||g.payload["achievementReference"].as_str().unwrap_or("").into());
            if payload["namespace"].as_str()!=Some(reference.as_str()) {return Ok(Err("成就命名空间与当前游戏关联不一致".into()));}
        }
    }
    if matches!(kind,ResourceType::TagMember|ResourceType::CollectionMember) {
        let category=if kind==ResourceType::TagMember {ResourceType::Tag}else{ResourceType::Collection};
        if resource(conn,identity.user_id,category,payload["categoryId"].as_str().unwrap_or("")).await?.is_none_or(|r|r.deleted) {return Ok(Err("分类已删除或不存在，旧操作不能复活分类".into()));}
    }
    if kind==ResourceType::CollectionOrder {
        let wanted:CollectionOrder=decode(payload).map_err(crate::ApiError::bad)?;
        let existing:Vec<String>=sqlx::query_scalar("SELECT id FROM sync_resources WHERE user_id=$1 AND kind='collection' AND NOT deleted").bind(identity.user_id).fetch_all(&mut *conn).await?;
        if wanted.ids.len()!=existing.len() || existing.iter().any(|id|!wanted.ids.iter().any(|v|v.to_string()==*id)) {return Ok(Err("收藏夹已变化，请基于当前列表重新排序".into()));}
    }
    if matches!(kind,ResourceType::OfficialSnapshot|ResourceType::OfficialPlaytime) {
        let provider=payload["provider"].as_str().unwrap_or("");
        if resource(conn,identity.user_id,ResourceType::PlatformAccount,provider).await?.is_none_or(|r|r.deleted||r.payload["accountId"]!=payload["accountId"]) {return Ok(Err("平台账号不匹配，已暂停该平台同步".into()));}
    }
    Ok(Ok(()))
}
