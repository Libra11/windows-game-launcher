import { el, button, icon } from '../lib/dom.js';
import { organizationPicker } from './organization-picker.js';
import { customSelect } from './custom-select.js';
import './organization.css';
let sequence=0;

export function tagFilter(state,actions) {
  const root=el('div','organization-tag-filter');const trigger=el('button','secondary');trigger.type='button';trigger.dataset.focusKey='tag-filter';
  trigger.append(icon('list'),el('span','',state.tagIds.length?`标签 · ${state.tagIds.length}`:'标签'));
  const menu=el('div','organization-popover');menu.popover='auto';menu.id=`organization-popover-${sequence++}`;
  menu.setAttribute('role','dialog');menu.setAttribute('aria-label','按标签筛选');trigger.setAttribute('popovertarget',menu.id);trigger.setAttribute('popovertargetaction','toggle');
  trigger.setAttribute('aria-haspopup','dialog');trigger.setAttribute('aria-expanded','false');
  const selected=new Set(state.tagIds);const picker=organizationPicker('tag',selected,actions,{create:false});
  const modes=el('div','organization-actions');let match=state.tagMatch;
  function paintModes(){for(const control of modes.children)control.setAttribute('aria-pressed',String(control.dataset.match===match));}
  for(const [value,label] of [['all','全部匹配'],['any','任意匹配']]){
    const control=button(label,'secondary',()=>{match=value;paintModes();});control.dataset.match=value;modes.append(control);
  }
  const footer=el('div','organization-actions');
  const close=()=>{if(menu.matches(':popover-open'))menu.hidePopover();trigger.focus({preventScroll:true});};
  footer.append(button('应用','primary',()=>{close();actions.organizationFilter([...selected],match);},'check'),button('清空','secondary',()=>{selected.clear();picker.paint();}),button('管理标签','secondary',()=>{
    close();state.bigScreen?actions.toast('请退出大屏模式后管理标签'):actions.organizationManage('tag');
  },'settings'));
  menu.append(modes,picker.element,footer);root.append(trigger,menu);paintModes();
  trigger.onclick=event=>{
    event.preventDefault();if(menu.matches(':popover-open')){close();return;}
    const rect=trigger.getBoundingClientRect();const width=Math.min(350,innerWidth-24);
    menu.style.width=`${width}px`;menu.style.left=`${Math.max(12,Math.min(rect.left,innerWidth-width-12))}px`;
    menu.style.maxHeight=`${Math.max(100,innerHeight-32)}px`;menu.showPopover({source:trigger});
    const height=menu.getBoundingClientRect().height;menu.style.top=`${Math.max(12,Math.min(rect.bottom+6,innerHeight-height-12))}px`;
    picker.element.querySelector('input')?.focus({preventScroll:true});
  };
  menu.addEventListener('toggle',event=>trigger.setAttribute('aria-expanded',String(event.newState==='open')));
  menu.addEventListener('keydown',event=>{if(event.key==='Escape'){event.preventDefault();event.stopPropagation();close();}});
  return root;
}
export function tagChips(state,actions) {
  const root=el('div','organization-chips');
  for(const id of state.tagIds){const item=state.organization.tags.find(item=>item.id===id);if(!item)continue;
    const chip=button(item.name,'organization-chip',()=>actions.organizationFilter(state.tagIds.filter(value=>value!==id),state.tagMatch),'close');chip.setAttribute('aria-label',`移除标签筛选 ${item.name}`);root.append(chip);
  }
  if(state.tagIds.length)root.append(button('清空标签','settings-link',()=>actions.organizationFilter([],state.tagMatch)));
  return root;
}
export function collectionSelect(state,actions) {
  const node=customSelect([['','全部收藏夹'],...state.organization.collections.map(item=>[item.id,item.name])],state.collectionId,'选择收藏夹','organization-collection-select');
  node.dataset.focusKey='collection-select';node.onchange=()=>actions.organizationCollection(node.value);
  if(state.bigScreen){const menu=node.querySelector('.custom-select-menu');menu.addEventListener('toggle',event=>{if(event.newState==='open')menu.querySelector('[aria-selected="true"]')?.focus({preventScroll:true});});}
  return node;
}
export function batchToolbar(state,games,actions) {
  const root=el('div','organization-batch-bar');root.setAttribute('aria-label','批量整理');
  root.append(el('strong','organization-selected-count',`已选择 ${state.organizationSelection.size} 款`),button('全选当前结果','secondary',()=>actions.organizationSelectAll(games.map(game=>game.id))),button('清空选择','secondary',actions.organizationClearSelection));
  for(const [kind,remove,label] of [['tag',false,'添加标签'],['tag',true,'移除标签'],['collection',false,'加入收藏夹'],['collection',true,'移出收藏夹']]){
    const control=button(label,'secondary',()=>actions.organizationBulk(kind,remove));control.dataset.organizationBatchAction='true';control.disabled=!state.organizationSelection.size;root.append(control);
  }
  root.append(button('退出整理','secondary',actions.organizationToggleBatch,'close'));return root;
}
export function selectionControl(game,state,actions) {
  const node=el('input','organization-game-check');node.type='checkbox';node.checked=state.organizationSelection.has(game.id);
  node.dataset.organizationGame=game.id;node.setAttribute('aria-label',`选择 ${game.title}`);node.dataset.focusKey=`select-game-${game.id}`;node.onchange=()=>actions.organizationToggleGame(game.id);return node;
}
export function mountOrganizationSidebar(host,state,actions) {
  const group=el('div','nav-group organization-nav');group.setAttribute('aria-label','收藏夹');
  const header=el('div','organization-nav-heading'),toggle=el('button','nav-group-label');toggle.type='button';toggle.append(icon('chevron'),el('span','','收藏夹'));toggle.setAttribute('aria-expanded','true');toggle.setAttribute('aria-controls','organization-sidebar-list');
  const list=el('div','organization-nav-list');list.id='organization-sidebar-list';
  toggle.onclick=()=>{list.hidden=!list.hidden;toggle.setAttribute('aria-expanded',String(!list.hidden));};
  const add=button('','icon-button',()=>actions.organizationNewCollection(),'plus');add.title='新建收藏夹';add.setAttribute('aria-label','新建收藏夹');
  const manage=button('','icon-button',()=>actions.organizationManage('collection'),'settings');manage.title='管理收藏夹';manage.setAttribute('aria-label','管理收藏夹');
  header.append(toggle,add,manage);group.append(header,list);host.insertBefore(group,host.children[1]);let key,expanded=false;
  const update=()=>{
    const next=JSON.stringify([state.organization.collections,state.organization.gameCollections,state.collectionId,expanded]);
    if(next!==key){key=next;list.replaceChildren();
      const items=expanded?[...state.organization.collections]:state.organization.collections.slice(0,4);
      const current=state.organization.collections.find(item=>item.id===state.collectionId);
      if(current&&!items.some(item=>item.id===current.id))items.push(current);
      for(const item of items){const control=button(item.name,'nav-item',()=>actions.organizationCollection(item.id),'folder');control.dataset.organizationCollection=item.id;control.title=item.name;control.append(el('span','nav-count',String(state.organization.byCollection.get(item.id)?.size||0)));list.append(control);}
      if(state.organization.collections.length>4){
        const more=button(expanded?'收起更多':`查看全部 · ${state.organization.collections.length}`,'organization-nav-more',()=>{expanded=!expanded;update();list.querySelector('.organization-nav-more')?.focus({preventScroll:true});},'chevron');
        more.setAttribute('aria-expanded',String(expanded));more.setAttribute('aria-controls',list.id);list.append(more);
      }
      if(!list.children.length)list.append(el('p','organization-nav-empty','创建清单，整理你的游戏。'));
    }
    for(const node of list.querySelectorAll('[data-organization-collection]')){const active=state.page==='library'&&state.collectionId===node.dataset.organizationCollection;node.classList.toggle('active',active);node.setAttribute('aria-current',active?'page':'false');}
  };
  return update;
}

// 勾选只更新选中标识和操作条，不重建整页卡片或封面。
export function patchOrganizationSelection(host,state) {
  host.querySelectorAll('[data-organization-game]').forEach(node=>{
    const selected=state.organizationSelection.has(node.dataset.organizationGame);
    if(node.checked!==selected)node.checked=selected;
    node.closest('.game-tile')?.classList.toggle('organization-selected',selected);
  });
  const count=host.querySelector('.organization-selected-count');if(count)count.textContent=`已选择 ${state.organizationSelection.size} 款`;
  host.querySelectorAll('[data-organization-batch-action]').forEach(node=>node.disabled=!state.organizationSelection.size);
}
