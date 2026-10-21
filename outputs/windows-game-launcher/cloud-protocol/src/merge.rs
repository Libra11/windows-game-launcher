use crate::*;

/// 纯业务合并；所有者、外键与唯一性检查由服务端事务负责。
pub fn decide(op: &Operation, current: Option<&Resource>, device: Uuid) -> Result<(Outcome, Value), String> {
    if op.base_revision < 0 { return Err("版本无效".into()); }
    if op.action == Action::Delete {
        if !matches!(op.resource_type, ResourceType::Tag | ResourceType::Collection) { return Err("此资源不支持删除，请使用归档或状态变更".into()); }
        return Ok(match current {
            Some(c) if c.deleted => (Outcome::Noop, c.payload.clone()),
            Some(c) if c.revision == op.base_revision => (Outcome::Applied, c.payload.clone()),
            _ => (Outcome::Conflict, Value::Null),
        });
    }
    validate(op.resource_type, &op.resource_id, &op.payload)?;
    if current.is_some_and(|c| c.deleted) { return Ok((Outcome::Conflict, Value::Null)); }
    if op.resource_type == ResourceType::PlaySession {
        let next: PlaySession = decode(&op.payload)?;
        if !next.historical && next.owner_device_id != device { return Err("只能推进本设备的游玩会话".into()); }
        if let Some(c) = current {
            let old: PlaySession = decode(&c.payload)?;
            if old.game_id != next.game_id || old.started_at != next.started_at || old.historical != next.historical || (!old.historical && old.owner_device_id != device) { return Err("游玩会话身份不匹配".into()); }
            if old.historical {
                // 历史副本可能只带总时长，不能以整包替换抹掉已知日期。
                let mut merged = if next.seconds > old.seconds { next.clone() } else { old.clone() };
                merged.owner_device_id = old.owner_device_id;
                if merged.ended_at.is_none() { merged.ended_at = old.ended_at.clone(); }
                for (day, seconds) in old.days.iter().chain(next.days.iter()) {
                    let known = merged.days.entry(day.clone()).or_default();
                    *known = (*known).max(*seconds);
                }
                let total = merged.days.values().try_fold(0u64, |sum, value| sum.checked_add(*value));
                if total.is_none_or(|sum| sum > merged.seconds) {
                    return Ok((Outcome::Conflict, c.payload.clone()));
                }
                let payload = serde_json::to_value(merged).map_err(|e| e.to_string())?;
                return Ok((if payload == c.payload { Outcome::Noop } else { Outcome::Applied }, payload));
            }
            if next.checkpoint < old.checkpoint { return Ok((Outcome::Noop, c.payload.clone())); }
            if next.checkpoint == old.checkpoint { return Ok((if next == old {Outcome::Noop} else {Outcome::Conflict}, c.payload.clone())); }
            if next.seconds < old.seconds || (old.ended_at.is_some() && next != old) || old.days.iter().any(|(day, seconds)| next.days.get(day).is_none_or(|x| x < seconds)) { return Err("计时不能倒退或重新打开已结束会话".into()); }
        }
        return Ok((Outcome::Applied, op.payload.clone()));
    }
    if let Some(c) = current {
        if c.payload == op.payload { return Ok((Outcome::Noop, c.payload.clone())); }
        match op.resource_type {
            ResourceType::Game | ResourceType::PlatformAccount | ResourceType::HistoricalBaseline => return Ok((Outcome::Conflict, c.payload.clone())),
            ResourceType::UnlockEvent => return Ok((Outcome::Noop, c.payload.clone())),
            ResourceType::AutomaticUnlock => {
                let mut old: Unlock = decode(&c.payload)?; let new: Unlock = decode(&op.payload)?;
                if let Some(time) = new.unlocked_at {
                    if old.unlocked_at.as_ref().is_none_or(|prior| chrono::DateTime::parse_from_rfc3339(&time).unwrap() < chrono::DateTime::parse_from_rfc3339(prior).unwrap()) { old.unlocked_at = Some(time); }
                }
                let payload = serde_json::to_value(old).map_err(|e| e.to_string())?;
                return Ok((if payload == c.payload {Outcome::Noop} else {Outcome::Applied}, payload));
            }
            _ => {}
        }
    }
    if current.map_or(0, |c| c.revision) != op.base_revision { return Ok((Outcome::Conflict, current.map_or(Value::Null, |c| c.payload.clone()))); }
    Ok((Outcome::Applied, op.payload.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn op(kind: ResourceType, payload: Value) -> Operation { Operation { operation_id:Uuid::new_v4(), resource_type:kind, resource_id:Uuid::new_v4().to_string(), action:Action::Set, base_revision:0, depends_on_operation_id:None, payload } }
    #[test] fn old_offline_edit_cannot_overwrite() {
        let o=op(ResourceType::Favorite,serde_json::json!({"value":true}));
        let c=Resource{resource_type:o.resource_type,resource_id:o.resource_id.clone(),revision:3,deleted:false,payload:serde_json::json!({"value":false}),device_id:Uuid::new_v4(),updated_at:String::new()};
        assert_eq!(decide(&o,Some(&c),Uuid::new_v4()).unwrap().0,Outcome::Conflict);
    }
    #[test] fn paths_and_tokens_are_not_accepted() {
        assert!(validate(ResourceType::Favorite,"test", &serde_json::json!({"value":true,"token":"secret"})).is_err());
        assert!(validate(ResourceType::Game,&Uuid::new_v4().to_string(), &serde_json::json!({"source":"local","sourceIdentity":"local-123","displayTitle":"game","metadataReference":"","achievementReference":"","exePath":"C:/game.exe"})).is_err());
    }
    #[test] fn sessions_are_owned_and_days_are_checked() {
        let device=Uuid::new_v4(); let s=PlaySession{game_id:Uuid::new_v4(),owner_device_id:device,checkpoint:1,started_at:"2026-10-21T00:00:00Z".into(),ended_at:None,seconds:120,days:[("2026-10-21".into(),120)].into(),utc_offset_seconds:0,historical:false};
        let o=op(ResourceType::PlaySession,serde_json::to_value(&s).unwrap());
        assert_eq!(decide(&o,None,device).unwrap().0,Outcome::Applied);
        assert!(decide(&o,None,Uuid::new_v4()).is_err());
        let mut invalid=s;invalid.seconds=121;
        assert!(validate(ResourceType::PlaySession,&o.resource_id,&serde_json::to_value(invalid).unwrap()).is_err());
    }
}

#[cfg(test)]
mod historical_regression_tests {
    use super::*;
    fn session() -> PlaySession {
        PlaySession {game_id:Uuid::new_v4(),owner_device_id:Uuid::new_v4(),checkpoint:1,
            started_at:"2026-10-20T00:00:00Z".into(),ended_at:Some("2026-10-20T00:05:00Z".into()),
            seconds:100,days:[("2026-10-20".into(),100)].into(),utc_offset_seconds:0,historical:true}
    }
    fn merge(old:&PlaySession,next:&PlaySession)->(Outcome,PlaySession) {
        let id=Uuid::new_v4().to_string();
        let current=Resource{resource_type:ResourceType::PlaySession,resource_id:id.clone(),revision:1,deleted:false,
            payload:serde_json::to_value(old).unwrap(),device_id:old.owner_device_id,updated_at:String::new()};
        let op=Operation{operation_id:Uuid::new_v4(),resource_type:ResourceType::PlaySession,resource_id:id,
            action:Action::Set,base_revision:1,depends_on_operation_id:None,payload:serde_json::to_value(next).unwrap()};
        let (outcome,value)=decide(&op,Some(&current),next.owner_device_id).unwrap();
        (outcome,decode(&value).unwrap())
    }
    #[test]
    fn larger_sparse_backup_retains_known_days_and_owner() {
        let old=session();let mut next=old.clone();next.seconds=120;next.days.clear();
        next.owner_device_id=Uuid::new_v4();next.ended_at=None;
        let (outcome,merged)=merge(&old,&next);
        assert_eq!(outcome,Outcome::Applied);assert_eq!(merged.seconds,120);
        assert_eq!(merged.days,old.days);assert_eq!(merged.owner_device_id,old.owner_device_id);
        assert_eq!(merged.ended_at,old.ended_at);
        assert_eq!(merge(&merged,&next).0,Outcome::Noop);
    }
    #[test]
    fn equal_length_backup_can_fill_missing_daily_facts() {
        let mut old=session();old.days.clear();let next=session_with_identity(&old);
        let (outcome,merged)=merge(&old,&next);
        assert_eq!(outcome,Outcome::Applied);assert_eq!(merged.days,next.days);
    }
    fn session_with_identity(old:&PlaySession)->PlaySession {
        let mut next=old.clone();next.days.insert("2026-10-20".into(),100);next
    }
    #[test]
    fn contradictory_daily_totals_conflict_without_losing_old_data() {
        let old=session();let mut next=old.clone();next.seconds=120;
        next.days=[("2026-10-21".into(),120)].into();
        let (outcome,merged)=merge(&old,&next);
        assert_eq!(outcome,Outcome::Conflict);assert_eq!(merged,old);
    }
}
