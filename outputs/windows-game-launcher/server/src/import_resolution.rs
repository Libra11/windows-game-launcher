use super::{Seed,Result,ApiError};
use serde::{Serialize,Deserialize};
use serde_json::{Value,json};
use std::collections::HashMap;
use youji_cloud_protocol::*;

#[derive(Clone,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct Resolution {pub key:String,pub choice:String,pub target_id:Option<String>,pub name:Option<String>}
#[derive(Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct CategoryMapping {pub kind:ResourceType,pub local_id:String,pub cloud_id:String}
pub struct Decisions {
    choices:HashMap<String,Resolution>,pub conflicts:Vec<Value>,pub mappings:Vec<CategoryMapping>,
}
impl Decisions {
    pub fn new(values:Vec<Resolution>)->Result<Self>{
        let mut choices=HashMap::new();for v in values{if choices.insert(v.key.clone(),v).is_some(){return Err(ApiError::bad("重复的导入解决项"));}}
        Ok(Self{choices,conflicts:vec![],mappings:vec![]})
    }
    pub fn skip(&self,key:&str)->bool{self.choices.get(key).is_some_and(|r|r.choice=="cloud")}
    pub fn key(seed:&Seed)->String{format!("{}:{}",seed.resource_type.key(),seed.resource_id)}
    pub fn category(&mut self,seed:&mut Seed,categories:&mut Vec<(ResourceType,String,Value)>)->Result<bool>{
        let key=Self::key(seed);let original=seed.resource_id.clone();
        if let Some(r)=self.choices.get(&key){
            if r.choice=="map"{
                let target=categories.iter().find(|(kind,id,_)|*kind==seed.resource_type&&Some(id)==r.target_id.as_ref()).ok_or_else(||ApiError::bad("选择的分类不存在"))?;
                self.mappings.push(CategoryMapping{kind:seed.resource_type,local_id:original,cloud_id:target.1.clone()});return Ok(false);
            }
            if r.choice=="rename"{seed.payload["name"]=json!(r.name.as_deref().unwrap_or("").trim());validate(seed.resource_type,&seed.resource_id,&seed.payload).map_err(ApiError::bad)?;}
        }
        let name=seed.payload["name"].as_str().unwrap_or("").to_lowercase();
        if let Some((_,_,other))=categories.iter().find(|(kind,id,p)|*kind==seed.resource_type&&*id!=seed.resource_id&&p["name"].as_str().unwrap_or("").to_lowercase()==name){
            let candidates:Vec<_>=categories.iter().filter(|(k,_,_)|*k==seed.resource_type).map(|(_,id,p)|json!({"id":id,"name":p["name"]})).collect();
            self.conflicts.push(json!({"key":key,"kind":seed.resource_type,"local":seed.payload,"cloud":other,"reason":"已有同名分类，请关联已有分类或重命名","choices":["map","rename"],"candidates":candidates}));return Ok(false);
        }
        categories.retain(|(k,id,_)|*k!=seed.resource_type||*id!=seed.resource_id);
        categories.push((seed.resource_type,seed.resource_id.clone(),seed.payload.clone()));Ok(true)
    }
    pub fn remap_category(&self,seed:&mut Seed){
        let kind=match seed.resource_type {ResourceType::TagMember=>ResourceType::Tag,ResourceType::CollectionMember|ResourceType::CollectionOrder=>ResourceType::Collection,_=>return};
        if seed.resource_type==ResourceType::CollectionOrder{
            if let Some(ids)=seed.payload["ids"].as_array_mut(){for id in ids.iter_mut(){if let Some(m)=self.mappings.iter().find(|m|m.kind==kind&&Some(m.local_id.as_str())==id.as_str()){*id=json!(m.cloud_id);}}let mut seen=std::collections::HashSet::new();ids.retain(|v|seen.insert(v.to_string()));}
        }else if let Some(m)=self.mappings.iter().find(|m|m.kind==kind&&Some(m.local_id.as_str())==seed.payload["categoryId"].as_str()){seed.payload["categoryId"]=json!(m.cloud_id);}
    }
    pub fn keep_local(&mut self,key:&str,seed:&Seed,current:&Resource)->bool{
        if !current.deleted&&current.payload==seed.payload{return false;}
        let allowed=!current.deleted&&!matches!(seed.resource_type,ResourceType::PlatformAccount|ResourceType::HistoricalBaseline|ResourceType::OfficialSnapshot|ResourceType::OfficialPlaytime);
        if let Some(choice)=self.choices.get(key){if choice.choice=="cloud"{return false;}if allowed&&matches!(choice.choice.as_str(),"local"|"rename"){return true;}}
        self.conflicts.push(json!({"key":key,"kind":seed.resource_type,"local":seed.payload,"cloud":current.payload,"deleted":current.deleted,"reason":if allowed{"本机与云端状态不同，请明确选择"}else{"此记录不能通过导入覆盖，请保留云端并重新采集"},"choices":if allowed{vec!["local","cloud"]}else{vec!["cloud"]}}));false
    }
}
