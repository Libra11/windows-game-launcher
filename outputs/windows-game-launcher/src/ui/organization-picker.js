import { el, button } from '../lib/dom.js';

// 选择器使用本地草稿，确认前不会修改游戏的归属。
export function organizationPicker(kind, selected, actions, {create=true}={}) {
  const root=el('section','organization-picker');
  const label=kind==='tag'?'标签':'收藏夹';
  const search=el('input','organization-search');search.type='search';search.placeholder=`搜索${label}`;search.setAttribute('aria-label',`搜索${label}`);
  const list=el('div','organization-picker-list');const status=el('p','form-hint');status.setAttribute('role','status');
  const entry=el('div','organization-create');
  const name=el('input');name.placeholder=`新${label}名称`;name.maxLength=80;name.setAttribute('aria-label',`新${label}名称`);
  const add=button('创建','secondary',async()=>{
    const id=await actions.organizationCreate(kind,name.value);selected.add(id);name.value='';search.value='';paint();
  },'plus');
  entry.append(name,add);
  function paint(){
    const term=search.value.trim().toLocaleLowerCase();
    const data=actions.organizationData();const items=data[kind==='tag'?'tags':'collections'];
    list.replaceChildren();
    for(const item of items.filter(item=>item.name.toLocaleLowerCase().includes(term))){
      const row=el('label','organization-option');const check=el('input');check.type='checkbox';check.checked=selected.has(item.id);check.dataset.focusKey=`organization-${kind}-${item.id}`;
      check.onchange=()=>{check.checked?selected.add(item.id):selected.delete(item.id);};
      row.append(check,el('span','',item.name));list.append(row);
    }
    status.textContent=items.length?(list.children.length?'':'没有匹配的分类'):`还没有${label}${create?'，可以在下面创建。':'。'}`;
  }
  search.oninput=paint;root.append(search,list,status);if(create)root.append(entry);
  paint();return {element:root,paint};
}
