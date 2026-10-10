import { el, button } from '../lib/dom.js';
import { modal, closeModal } from './modal.js';
import { organizationPicker } from './organization-picker.js';
import './organization.css';

export function organizeGameDialog(game,actions) {
  const data=actions.organizationData();
  const tags=new Set(data.byGameTags.get(game.id)||[]),collections=new Set(data.byGameCollections.get(game.id)||[]);
  const body=el('div','organization-dialog');
  body.append(el('h3','','标签'));
  const tagPicker=organizationPicker('tag',tags,actions);body.append(tagPicker.element,el('h3','','收藏夹'));
  const collectionPicker=organizationPicker('collection',collections,actions);body.append(collectionPicker.element);
  const dialog=modal('整理游戏',`${game.title} · 新建分类立即保存，游戏归属在确认后保存。`,body);
  body.append(button('保存归属','primary',async()=>{
    await actions.organizationSave(game.id,[...tags],[...collections]);await closeModal(dialog);
  },'check'));
}
export function bulkOrganizationDialog(kind,remove,gameIds,actions) {
  const selected=new Set(),body=el('div','organization-dialog');
  const label=kind==='tag'?'标签':'收藏夹';
  body.append(organizationPicker(kind,selected,actions,{create:!remove}).element);
  const dialog=modal(`${remove?'移除':'添加'}${label}`,`将对选中的 ${gameIds.length} 款游戏操作，保留其他归属。`,body);
  body.append(button('确认','primary',async()=>{
    if(!selected.size){actions.toast('请先选择分类');return;}
    await actions.organizationBatch(kind,remove,gameIds,[...selected]);await closeModal(dialog);
  },'check'));
}
export { manageOrganizationDialog } from './organization-manager.js';
