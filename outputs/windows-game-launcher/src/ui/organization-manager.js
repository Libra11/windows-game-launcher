import { el, button } from '../lib/dom.js';
import { modal, closeModal } from './modal.js';
import './organization-manager.css';

function confirmDelete(item,kind,actions,done) {
  const body=el('div','organization-dialog');
  body.append(el('p','','只移除分类及归属关系，游戏、时长和成就记录都会保留。'));
  const dialog=modal(`删除「${item.name}」`,'删除后无法直接撤销。',body);
  const buttons=el('div','organization-actions');
  buttons.append(button('取消','secondary',()=>closeModal(dialog)),button('删除分类','secondary remove-game-action',async()=>{
    await actions.organizationDelete(kind,item.id);await closeModal(dialog);done();
  },'trash'));body.append(buttons);
}

export function manageOrganizationDialog(kind,actions) {
  const collection=kind==='collection',label=collection?'收藏夹':'标签';
  const body=el('div','organization-manager-body'),toolbar=el('div','organization-manager-toolbar');
  const search=el('input');search.type='search';search.placeholder=`搜索${label}`;search.setAttribute('aria-label',`搜索${label}`);
  const create=button(`新建${label}`,'primary organization-manager-add',()=>{
    createForm.hidden=!createForm.hidden;create.setAttribute('aria-expanded',String(!createForm.hidden));
    if(!createForm.hidden)newName.focus();
  },'plus');create.setAttribute('aria-expanded','false');
  toolbar.append(search,create);
  const createForm=el('form','organization-manager-create');createForm.hidden=true;
  const newName=el('input');newName.placeholder=`输入${label}名称`;newName.maxLength=80;newName.setAttribute('aria-label',`新${label}名称`);
  const confirmCreate=button('创建','primary',()=>mutate(async()=>{
    await actions.organizationCreate(kind,newName.value);newName.value='';search.value='';createForm.hidden=true;create.setAttribute('aria-expanded','false');
  }));
  createForm.append(newName,confirmCreate,button('取消','organization-manager-text',()=>{createForm.hidden=true;create.setAttribute('aria-expanded','false');create.focus();}));
  createForm.onsubmit=event=>{event.preventDefault();confirmCreate.click();};
  const rows=el('div','organization-manager-list');rows.setAttribute('aria-label',`${label}列表`);
  const footer=el('div','organization-manager-footer'),summary=el('span');
  footer.append(summary,el('span','',collection?'侧栏显示顺序可用箭头调整':'标签可用于组合筛选'));
  body.append(toolbar,createForm,rows,footer);
  const dialog=modal(`管理${label}`,collection?'把游戏整理成清单，按自己的节奏排列。':'为游戏添加标记，让筛选更轻松。',body);
  dialog.classList.add('organization-manager-modal');
  let busy=false,editingId='',editingValue='';
  function iconAction(title,glyph,callback,key,className=''){
    const node=button('','organization-manager-icon '+className,callback,glyph);node.title=title;node.setAttribute('aria-label',title);node.dataset.managerFocus=key;return node;
  }
  async function mutate(task,focusKey){
    if(busy)return;busy=true;body.setAttribute('aria-busy','true');
    body.querySelectorAll('button,input').forEach(node=>node.disabled=true);
    const scroll=rows.scrollTop;
    try{await task();}finally{
      busy=false;body.removeAttribute('aria-busy');toolbar.querySelectorAll('button,input').forEach(node=>node.disabled=false);createForm.querySelectorAll('button,input').forEach(node=>node.disabled=false);
      if(dialog.open){paint();rows.scrollTop=scroll;if(focusKey)[...rows.querySelectorAll('[data-manager-focus]')].find(node=>node.dataset.managerFocus===focusKey&&!node.disabled)?.focus({preventScroll:true});}
    }
  }
  function paint(){
    const data=actions.organizationData(),items=data[collection?'collections':'tags'];
    const term=search.value.trim().toLocaleLowerCase();rows.replaceChildren();
    const visible=items.filter(item=>item.name.toLocaleLowerCase().includes(term));
    summary.textContent=term?`${visible.length} / ${items.length} 个${label}`:`${items.length} 个${label}`;
    if(!visible.length){const empty=el('div','organization-manager-empty');empty.append(el('strong','',items.length?'没有匹配的分类':`还没有${label}`),el('span','',items.length?'试试其他关键词':`点击右上角，创建你的第一个${label}`));rows.append(empty);}
    for(const item of visible){
      const index=items.findIndex(entry=>entry.id===item.id),row=el('div','organization-manager-entry');
      row.append(el('span','organization-manager-position',String(index+1).padStart(2,'0')));
      const copy=el('div','organization-manager-copy'),tools=el('div','organization-manager-tools');
      if(editingId===item.id){
        const input=el('input','organization-manager-name-input');input.value=editingValue;input.maxLength=80;input.setAttribute('aria-label',`修改 ${item.name} 的名称`);input.dataset.managerFocus=`name-${item.id}`;
        input.oninput=()=>editingValue=input.value;
        const save=()=>mutate(async()=>{await actions.organizationRename(kind,item.id,editingValue);editingId='';},`edit-${item.id}`);
        const cancel=()=>{editingId='';paint();[...rows.querySelectorAll('[data-manager-focus]')].find(node=>node.dataset.managerFocus===`edit-${item.id}`)?.focus({preventScroll:true});};
        input.onkeydown=event=>{if(event.key==='Enter'){event.preventDefault();save().catch(()=>{});}else if(event.key==='Escape'){event.preventDefault();event.stopPropagation();cancel();}};
        copy.append(input);
        tools.append(iconAction('保存名称','check',save,`save-${item.id}`,'organization-manager-save'),iconAction('取消编辑','close',cancel,`cancel-${item.id}`));
        row.classList.add('editing');
      }else{
        copy.append(el('strong','',item.name));
        const count=collection?data.byCollection.get(item.id)?.size||0:data.gameTags.filter(([,id])=>id===item.id).length;
        copy.append(el('span','',`${count} 款游戏`));
        tools.append(iconAction(`重命名 ${item.name}`,'edit',()=>{
          editingId=item.id;editingValue=item.name;paint();const input=rows.querySelector('.organization-manager-name-input');input?.focus({preventScroll:true});input?.select();
        },`edit-${item.id}`));
        if(collection)for(const [offset,title] of [[-1,'上移'],[1,'下移']]){
          const move=iconAction(`${title} ${item.name}`,'arrow',()=>mutate(async()=>{
            const ids=items.map(entry=>entry.id);[ids[index],ids[index+offset]]=[ids[index+offset],ids[index]];await actions.organizationReorder(ids);
          },`move-${offset}-${item.id}`),`move-${offset}-${item.id}`,offset<0?'move-up':'move-down');
          move.disabled=!!term||index+offset<0||index+offset>=items.length;if(term)move.title='清空搜索后调整顺序';tools.append(move);
        }
        tools.append(iconAction(`删除 ${item.name}`,'trash',()=>confirmDelete(item,kind,actions,paint),`delete-${item.id}`,'organization-manager-delete'));
      }
      row.append(copy,tools);rows.append(row);
    }
  }
  search.oninput=()=>{editingId='';paint();};paint();search.focus({preventScroll:true});return dialog;
}
