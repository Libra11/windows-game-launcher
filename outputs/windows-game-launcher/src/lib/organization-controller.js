import { command, onOrganizationChange, preview } from './bridge.js';
import { emptyOrganization, indexOrganization, reconcileOrganization, clearOrganizationSelection, createOrganizationLoader } from './library-organization.js';
import { organizeGameDialog, bulkOrganizationDialog, manageOrganizationDialog } from '../ui/organization-dialogs.js';
import { modal, closeModal } from '../ui/modal.js';
import { el, button } from './dom.js';

export function createOrganizationController(state,render,actions,onNavigate=()=>{}) {
  Object.assign(state,{organization:indexOrganization(emptyOrganization()),organizationRevision:0,collectionId:'',tagIds:[],tagMatch:'all',organizationBatchMode:false,organizationSelection:new Set()});
  let unlisten;
  const loader=createOrganizationLoader(()=>command('get_library_organization'),data=>{
    state.organization=data;state.organizationRevision++;reconcileOrganization(state);render();
  });
  const refresh=loader.refresh;
  const mutation=async(name,args)=>{await actions.run(name,args);await refresh();};
  function changeFilter(task){onNavigate();clearOrganizationSelection(state);task();state.page='library';state.selectedId='';render();}
  const exposed={
    organizationData:()=>state.organization,
    organizationCreate:async(kind,name)=>{const id=await actions.run(`create_library_${kind}`,{name});await refresh();return id;},
    organizationRename:(kind,id,name)=>mutation(`rename_library_${kind}`,{id,name}),
    organizationDelete:(kind,id)=>mutation(`delete_library_${kind}`,{id}),
    organizationReorder:ids=>mutation('reorder_library_collections',{ids}),
    organizationSave:(gameId,tagIds,collectionIds)=>mutation('set_game_organization',{gameId,tagIds,collectionIds}),
    organizationBatch:(kind,remove,gameIds,ids)=>mutation('batch_update_game_organization',{changes:{gameIds,addTagIds:kind==='tag'&&!remove?ids:[],removeTagIds:kind==='tag'&&remove?ids:[],addCollectionIds:kind==='collection'&&!remove?ids:[],removeCollectionIds:kind==='collection'&&remove?ids:[]}}),
    organizationFilter:(ids,match)=>changeFilter(()=>{state.tagIds=ids;state.tagMatch=match;}),
    organizationCollection:id=>changeFilter(()=>{state.collectionId=id;state.filter='all';state.bigCollection='all';}),
    organizationResetFilters:()=>changeFilter(()=>{state.collectionId='';state.tagIds=[];state.installedOnly=false;state.search='';document.querySelector('#search').value='';}),
    organize:game=>state.bigScreen?actions.toast('请退出大屏模式后整理游戏'):organizeGameDialog(game,actions),
    organizationManage:kind=>manageOrganizationDialog(kind,actions),
    organizationToggleBatch:()=>{state.organizationBatchMode=!state.organizationBatchMode;clearOrganizationSelection(state);render();},
    organizationToggleGame:id=>{state.organizationSelection.has(id)?state.organizationSelection.delete(id):state.organizationSelection.add(id);render(`select-game-${id}`);},
    organizationSelectAll:ids=>{for(const id of ids)state.organizationSelection.add(id);render();},
    organizationClearSelection:()=>{clearOrganizationSelection(state);render();},
    organizationBulk:(kind,remove)=>{if(state.organizationSelection.size)bulkOrganizationDialog(kind,remove,[...state.organizationSelection],actions);},
    organizationNewCollection:()=>{
      const body=el('div','organization-dialog'),input=el('input');input.placeholder='收藏夹名称';input.setAttribute('aria-label','收藏夹名称');body.append(input);
      const dialog=modal('新建收藏夹','创建跨平台的游戏清单。',body);body.append(button('创建','primary',async()=>{await exposed.organizationCreate('collection',input.value);await closeModal(dialog);},'plus'));input.focus();
    },
  };
  Object.assign(actions,exposed);
  async function start(){
    if(!preview)unlisten=await onOrganizationChange(()=>refresh().catch(()=>{}));
    await refresh();
  }
  if(import.meta.hot)import.meta.hot.dispose(()=>{loader.dispose();unlisten?.();});
  return {start,refresh};
}
