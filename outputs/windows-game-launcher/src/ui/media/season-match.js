import { el, button } from '../../lib/dom.js';
import { modal, closeModal } from '../modal.js';
import { displayMediaPath } from '../../lib/media-path.js';
import { metadataSearchTitle } from '../../lib/media-detail-model.js';
import { parseEpisodeNumbers, suggestedEpisodes } from '../../lib/media-season-match.js';
import './season-match.css';

const label = row => `第 ${row.episodeNumber} 集 · ${row.title}`;
const oldTitle = row => [row.seriesTitle, row.parentTitle, row.title].filter(Boolean).join(' · ');
export function seasonMatchDialog(controller, target) {
  const content = el('div', 'media-season-match form');
  const searchForm = el('form', 'input-row');
  const query = el('input'); query.type = 'search';query.required = true;query.maxLength = 200;
  query.placeholder = '搜索电视剧，选择具体季';query.setAttribute('aria-label','电视剧名称');
  const pathNames=displayMediaPath(target.path).split(/[\\/]/).filter(Boolean);
  const folderName=pathNames.at(-1)||'';
  const genericSeason=/^(?:s\d+|season[ ._-]*\d+|第[\d一二三四五六七八九十]+季)$/i.test(folderName);
  query.value = target.item ? metadataSearchTitle(target.item, controller.state.data.items) : genericSeason ? pathNames.at(-2)||folderName : folderName;
  const recursive = el('input');recursive.type='checkbox';recursive.setAttribute('aria-label','包含子目录');
  const scope = el('label','media-season-scope');scope.append(recursive,document.createTextNode('包含子目录中的文件'));
  const status = el('p','form-hint');status.setAttribute('role','status');
  const error = el('p','media-error');error.setAttribute('role','alert');
  const results = el('div','media-season-candidates');
  const previewHost = el('div','media-season-preview');const reviewHost=el('div','media-season-review');
  let dialog, generation=0, inspection=0, preview=null, inputRows=[], review=null;
  let saving=false, composing=false;
  query.addEventListener('compositionstart',()=>{composing=true;});
  query.addEventListener('compositionend',()=>{composing=false;});
  query.addEventListener('keydown',event=>{if(event.key==='Enter'&&(event.isComposing||event.keyCode===229))event.preventDefault();});
  const request=(action,input={})=>controller.run('media_match_season',{action,sourceId:target.sourceId,path:target.path,recursive:recursive.checked,...input},false);
  function discard(value=preview) { if(value?.token) request('discard',{token:value.token}).catch(()=>{}); }
  function clearReview() { generation++;review=null;reviewHost.replaceChildren(); }
  function invalidate() { generation++;discard();preview=null;previewHost.replaceChildren();clearReview();error.textContent=''; }
  function scopeChanged() { invalidate();results.replaceChildren();inspect(); }
  recursive.onchange=scopeChanged;
  query.oninput=()=>{invalidate();results.replaceChildren();};
  async function inspect() {
    const version=++inspection;status.textContent='正在读取已扫描文件…';
    try {const value=await request('inspect');if(dialog.open&&version===inspection)status.textContent=`范围内有 ${value.count} 个已扫描文件。未扫描的新文件请先扫描媒体目录。`;}
    catch(e){if(dialog.open&&version===inspection){status.textContent='';error.textContent=String(e);}}
  }
  async function search() {
    if(saving||composing)return;
    invalidate();const version=generation;
    results.replaceChildren(el('p','form-hint','正在搜索电视剧与季…'));
    try {
      const candidates=await controller.run('media_search_metadata',{query:query.value.trim(),kind:'series'},false);
      if(!dialog.open||version!==generation)return;
      results.replaceChildren();
      for(const candidate of candidates) {
        if(candidate.providerType!=='tv'||candidate.seasonNumber==null)continue;
        const choice=button('', 'secondary media-season-choice',()=>choose(candidate));
        choice.append(el('strong','',`${candidate.seriesTitle||candidate.title} · ${candidate.seasonTitle||`第 ${candidate.seasonNumber} 季`}`),el('small','form-hint',candidate.year||''));
        results.append(choice);
      }
      if(!results.children.length)results.append(el('p','form-hint','没有找到具体季，请更换名称搜索。'));
    }catch(e){if(dialog.open&&version===generation){results.replaceChildren();error.textContent=String(e);}}
  }
  async function choose(candidate) {
    if(saving)return;
    invalidate();const version=generation;
    previewHost.replaceChildren(el('p','form-hint','正在获取季资料和文件对应建议…'));
    try {
      const value=await request('preview',{candidate});
      if(!dialog.open||version!==generation){discard(value);return;}
      preview=value;buildPreview();previewHost.scrollIntoView({block:'start'});
    }catch(e){if(dialog.open&&version===generation){previewHost.replaceChildren();error.textContent=String(e);}}
  }
  function mappings() {
    return inputRows.flatMap(({file,input})=>{
      if(file.disabled)return [];
      const numbers=parseEpisodeNumbers(input.value,preview.episodes);
      return numbers.length?[{resourceId:file.resourceId,episodeNumbers:numbers}]:[];
    });
  }
  function buildPreview() {
    previewHost.replaceChildren();inputRows=[];
    previewHost.append(el('h3','',`${preview.title} · ${preview.seasonTitle||`第 ${preview.seasonNumber} 季`}`),
      el('p','form-hint','填写目标集号，留空即跳过；合并文件可填写 1-2 或 1,2。相同分集可以关联多个版本。'));
    const episodeList=el('details');episodeList.append(el('summary','',`查看本季 ${preview.episodes.length} 集`));
    for(const episode of preview.episodes)episodeList.append(el('p','form-hint',label(episode)));
    previewHost.append(episodeList);
    const offset=el('input');offset.type='number';offset.step='1';offset.value='0';offset.setAttribute('aria-label','集号偏移');
    const controls=el('div','media-action-row');
    const count=el('span','form-hint');
    const overrideSeason=el('input');overrideSeason.type='checkbox';
    const overrideLabel=el('label','media-season-scope');overrideLabel.append(overrideSeason,document.createTextNode('已确认所选季，允许按文件集号更正季号差异'));
    const update=()=>{clearReview();error.textContent='';try{count.textContent=`已选择 ${mappings().length} / ${inputRows.length} 个文件`;}catch(e){count.textContent=String(e);}};
    controls.append(el('span','','集号偏移'),offset,button('重新建议集号','secondary',()=>{
      const value=Number(offset.value);if(!Number.isInteger(value)){error.textContent='偏移须为整数';return;}
      for(const row of inputRows)row.input.value=suggestedEpisodes(row.file,preview.episodes,value,overrideSeason.checked).join(',');update();
    }),button('全部跳过','secondary',()=>{inputRows.forEach(row=>row.input.value='');update();}),count);
    previewHost.append(controls,overrideLabel);
    const table=el('table','media-season-table');
    const head=el('thead');const header=el('tr');
    for(const text of ['本地文件','原关联','目标分集'])header.append(el('th','',text));
    head.append(header);table.append(head);
    const body=el('tbody');
    const listId=`media-episodes-${Math.random().toString(36).slice(2)}`;
    const dataList=el('datalist');dataList.id=listId;
    for(const episode of preview.episodes){const option=el('option');option.value=String(episode.episodeNumber);option.label=label(episode);dataList.append(option);}
    for(const file of preview.files) {
      const row=el('tr');const name=el('td');name.append(el('strong','',file.name));
      const path=el('small','media-season-path',displayMediaPath(file.path));path.title=path.textContent;name.append(path);
      if(file.warning)name.append(el('small','media-season-warning',file.warning));
      const current=el('td','',file.current.map(oldTitle).join('；')||'未关联');
      const next=el('td');const input=el('input');input.type='text';input.placeholder='跳过';input.maxLength=200;input.value=(file.suggestedEpisodes||[]).join(',');input.disabled=file.disabled;
      input.setAttribute('list',listId);input.setAttribute('aria-label',`${file.name} 的目标集号`);
      const description=el('small','form-hint');
      input.oninput=()=>{try{const numbers=parseEpisodeNumbers(input.value,preview.episodes);description.textContent=numbers.map(n=>label(preview.episodes.find(e=>e.episodeNumber===n))).join('；');input.setCustomValidity('');}
        catch(e){description.textContent=String(e);input.setCustomValidity(String(e));}update();};
      next.append(input,description);row.append(name,current,next);body.append(row);inputRows.push({file,input});
    }
    table.append(body);const wrap=el('div','media-season-table-wrap');wrap.append(table);previewHost.append(wrap,dataList);
    previewHost.append(button('预览更正影响','primary',async()=>{
      error.textContent='';clearReview();try{const rows=mappings();if(!rows.length)throw Error('请至少选择一个文件的目标分集');
        await loadReview(rows);
      }catch(e){error.textContent=String(e);}
    }));update();
  }
  async function loadReview(rows,recordTargets={}) {
    const token=preview.token,version=++generation;
    let value;
    try { value=await request('review',{token,mappings:rows,recordTargets}); }
    catch(e) { if(dialog.open&&version===generation&&preview?.token===token)throw e;return; }
    if(!dialog.open||version!==generation||preview?.token!==token)return;
    review=value;buildReview(rows,recordTargets);
  }
  function buildReview(rows,recordTargets) {
    reviewHost.replaceChildren();
    reviewHost.append(el('h3','','确认本次更正'),el('p','form-hint',`将关联 ${rows.length} 个文件。续播位置保留；只处理本次选择的文件。`));
    const transfers={...recordTargets};
    for(const effect of review.effects) {
      const row=el('div','media-season-effect');row.append(el('strong','',effect.title),el('span','form-hint',effect.reason));
      if(effect.options?.length) {
        const select=el('select');select.setAttribute('aria-label',`${effect.title} 的记录归属`);
        const keep=el('option','','保留原条目及记录');keep.value='keep';select.append(keep);
        for(const option of effect.options){const node=el('option','',`迁移到 ${option.title}`);node.value=option.key;select.append(node);}
        select.value=effect.targetKey||'keep';
        select.onchange=async()=>{transfers[effect.itemId]=select.value;error.textContent='';reviewHost.replaceChildren(el('p','form-hint','正在重新计算记录迁移…'));
          try{await loadReview(rows,transfers);}catch(e){error.textContent=String(e);}};
        row.append(select);
      }
      reviewHost.append(row);
    }
    const resolutions={};
    for(const conflict of review.conflicts) {
      const field=el('label','media-season-conflict');field.append(el('span','',`${conflict.title} · ${conflict.label}存在不同记录，请选择保留哪一份`));
      const select=el('select');select.setAttribute('aria-label',`${conflict.title} 的${conflict.label}冲突`);
      const empty=el('option','','请选择');empty.value='';select.append(empty);
      for(const choice of conflict.choices){const option=el('option','',`${choice.title}：${choice.value}`);option.value=choice.id;select.append(option);}
      select.onchange=()=>{resolutions[conflict.key]=select.value;};field.append(select);reviewHost.append(field);
    }
    reviewHost.append(el('p','form-hint','确认后，记录迁移且没有其他文件支撑的旧条目会清理；仍有其他文件或选择保留的旧条目继续存在。'),
      button('确认更正','primary',async()=>{
        if(saving)return;
        error.textContent='';
        if(review.conflicts.some(conflict=>!resolutions[conflict.key])){error.textContent='请先处理所有记录冲突';return;}
        saving=true;content.inert=true;content.setAttribute('aria-busy','true');status.textContent='正在保存更正…';
        try {
          const result=await request('apply',{token:preview.token,reviewId:review.reviewId,resolutions});preview=null;
          await closeModal(dialog);await controller.refresh();await controller.refreshLocalFiles(target.sourceId);
          controller.toast(`已完成 ${result.matched} 个文件的分集关联`);
        } catch(e){error.textContent=`${e}。请重新选择季并预览后再确认。`;discard();preview=null;previewHost.replaceChildren();reviewHost.replaceChildren();}
        finally{saving=false;content.inert=false;content.removeAttribute('aria-busy');status.textContent='';}
      }));
    reviewHost.scrollIntoView({block:'start'});
  }
  searchForm.append(query,button('搜索电视剧','primary',search,'search'));
  searchForm.onsubmit=event=>{event.preventDefault();if(!event.isComposing)search();};
  content.append(el('p','media-season-path',displayMediaPath(target.path)),scope,status,searchForm,error,results,previewHost,reviewHost);
  dialog=modal('匹配／更正本季剧集','选择作品与季 → 核对文件分集 → 预览更正影响 → 确认保存',content);
  dialog.addEventListener('close',()=>{generation++;inspection++;discard();},{once:true});
  inspect();return dialog;
}
