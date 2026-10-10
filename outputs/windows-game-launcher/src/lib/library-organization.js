export const emptyOrganization = () => ({tags:[],collections:[],gameTags:[],gameCollections:[]});
export function indexOrganization(data) {
  const byGameTags=new Map(),byGameCollections=new Map(),byCollection=new Map();
  for(const [game,id] of data.gameTags){if(!byGameTags.has(game))byGameTags.set(game,new Set());byGameTags.get(game).add(id);}
  for(const [game,id] of data.gameCollections){
    if(!byGameCollections.has(game))byGameCollections.set(game,new Set());byGameCollections.get(game).add(id);
    if(!byCollection.has(id))byCollection.set(id,new Set());byCollection.get(id).add(game);
  }
  return {...data,byGameTags,byGameCollections,byCollection};
}
export function organizationMatches(game,organization,collectionId='',tagIds=[],tagMatch='all') {
  if(collectionId&&!organization?.byGameCollections.get(game.id)?.has(collectionId))return false;
  if(!tagIds.length)return true;
  const tags=organization?.byGameTags.get(game.id);
  return tagMatch==='any'?tagIds.some(id=>tags?.has(id)):tagIds.every(id=>tags?.has(id));
}
export function organizationOptions(state) {
  return {organization:state.organization,collectionId:state.collectionId||'',tagIds:state.tagIds||[],tagMatch:state.tagMatch||'all'};
}
export function clearOrganizationSelection(state) {state.organizationSelection?.clear();}
export function reconcileOrganization(state) {
  if(state.collectionId&&!state.organization.collections.some(item=>item.id===state.collectionId)){
    state.collectionId='';state.filter='all';state.bigCategory='all';state.bigCollection='all';clearOrganizationSelection(state);
  }
  const known=new Set(state.organization.tags.map(item=>item.id));
  const next=(state.tagIds||[]).filter(id=>known.has(id));
  if(next.length!==(state.tagIds||[]).length)clearOrganizationSelection(state);
  state.tagIds=next;
  const games=new Set(state.games.map(game=>game.id));
  for(const id of state.organizationSelection||[])if(!games.has(id))state.organizationSelection.delete(id);
}

// 独立分类事件合并读取；旧读取结束后再读取新快照，避免迟到结果覆盖。
export function createOrganizationLoader(read,commit) {
  let pending,again=false,disposed=false,snapshot=JSON.stringify(emptyOrganization());
  function refresh(){
    again=true;if(pending)return pending;
    pending=(async()=>{do{
      again=false;const data=await read();if(disposed)return;
      const next=JSON.stringify(data);if(next!==snapshot){snapshot=next;commit(indexOrganization(data));}
    }while(again);})().finally(()=>pending=null);return pending;
  }
  return {refresh,dispose:()=>{disposed=true;}};
}
