import { el, button } from '../lib/dom.js';
import { chooseFile } from '../lib/bridge.js';
import { backupPathLabels, needsRelocation } from '../lib/backup.js';

export function backupPaths(inspection,actions,changed) {
  const element=el('section','backup-paths');
  element.append(el('h3','','重新定位游戏路径'),el('p','settings-description','目录映射保留相对路径，只修改游迹中的程序和自定义记录位置。游戏文件与历史证据不会移动。'));
  const mappings=el('div','backup-mappings'),rows=el('div','backup-path-list'),status=el('p','settings-description');
  let entries=[],overrides=new Map(),paths=inspection.paths,dirty=false,busy=false,showAll=false,disposed=false,disabled=false;
  const notify=()=>changed({dirty,busy,remaining:paths.filter(needsRelocation).length});
  function edit(){dirty=true;notify();status.textContent='路径映射已修改，请点击「预览路径」确认。';}
  function mappingRow() {
    const root=el('div','backup-mapping-row');
    const from=el('input'),to=el('input');from.placeholder='原目录，例如 E:/Games';to.placeholder='新目录，例如 D:/Games';
    from.setAttribute('aria-label','备份中的原目录');to.setAttribute('aria-label','目标电脑的新目录');
    const entry={from,to};entries.push(entry);
    from.oninput=edit;to.oninput=edit;
    const remove=button('移除','settings-link',()=>{entries=entries.filter(item=>item!==entry);root.remove();edit();});
    root.append(from,el('span','backup-mapping-arrow','→'),to,remove);mappings.append(root);
  }
  function relocation() {
    return {mappings:entries.filter(item=>item.from.value.trim()||item.to.value.trim()).map(item=>({from:item.from.value.trim(),to:item.to.value.trim()})),overrides:[...overrides.values()]};
  }
  async function preview() {
    if(busy)return;busy=true;notify();renderRows();status.textContent='正在检查程序与记录路径…';
    try {
      const next=await actions.run('preview_backup_paths',{preparationId:inspection.preparationId,relocation:relocation()});
      if(disposed)return;paths=next;dirty=false;status.textContent=`${paths.filter(needsRelocation).length} 款游戏仍待定位，可先恢复数据，之后再补选。`;
    }catch(error){if(!disposed)status.textContent=String(error);}
    finally{busy=false;if(!disposed){renderRows();notify();}}
  }
  async function pick(path,record=false) {
    const selected=await chooseFile(record?{multiple:false,title:'选择自定义成就记录文件'}:{multiple:false,title:'选择游戏程序',filters:[{name:'游戏程序',extensions:['exe']}]});
    if(!selected||disposed)return;
    const previous=overrides.get(path.gameId)||{gameId:path.gameId,exePath:path.exePath};
    overrides.set(path.gameId,{...previous,...(record?{customUnlockPath:selected}:{exePath:selected})});
    dirty=true;notify();await preview();
  }
  function renderRows() {
    rows.replaceChildren();
    const visible=showAll?paths:paths.filter(needsRelocation);
    if(!visible.length)rows.append(el('p','settings-description','当前没有待处理的游戏路径。'));
    for(const path of visible){
      const row=el('div','backup-path-row'),copy=el('div','backup-path-copy'),tools=el('div','settings-tools');
      copy.append(el('strong','',path.title),el('span','',path.exePath||'通过游戏平台客户端启动'),el('small','',backupPathLabels[path.status]||path.status));
      if(path.customUnlockPath)copy.append(el('span','',`记录：${path.customUnlockPath}`),el('small','',`记录路径：${backupPathLabels[path.recordStatus]||path.recordStatus}`));
      const program=button('选择程序','secondary',()=>pick(path),'folder');program.disabled=busy||disabled;tools.append(program);
      if(path.customUnlockPath){const record=button('选择记录','settings-link',()=>pick(path,true));record.disabled=busy||disabled;tools.append(record);}
      row.append(copy,tools);rows.append(row);
    }
  }
  const tools=el('div','settings-tools');
  const check=button('预览路径','secondary',preview,'search');
  tools.append(button('添加目录映射','secondary',()=>{mappingRow();edit();},'plus'),check,
    button('切换全部／待处理','settings-link',()=>{showAll=!showAll;renderRows();}));
  element.append(mappings,tools,status,rows);renderRows();notify();
  return {element,relocation,setDisabled(value){disabled=value;element.querySelectorAll('button,input').forEach(node=>{node.disabled=disabled||busy;});},dispose(){disposed=true;}};
}
