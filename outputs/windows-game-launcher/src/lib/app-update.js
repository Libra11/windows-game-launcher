export const updatePhases={idle:'尚未检查',cached:'已有下载缓存，检查更新后可确认是否复用',checking:'正在检查更新…',current:'当前已是最新正式版',available:'有新版本可用',downloading:'正在下载更新…',verifying:'正在校验签名…',ready:'更新已下载，等待安装',preparing:'正在保存记录并准备安装…',installing:'正在启动安装器…',error:'更新操作未完成'};
export const updateBusy=status=>['checking','downloading','verifying','preparing','installing'].includes(status?.phase);
export const updateAvailable=status=>!!status?.preparationId&&!!status?.version&&status.version!==status.currentVersion;
export function downloadSize(bytes){if(!Number.isFinite(bytes)||bytes<0)return '—';return bytes<1048576?`${(bytes/1024).toFixed(0)} KiB`:`${(bytes/1048576).toFixed(1)} MiB`;}
export function downloadPercent(status){return status.total>0?Math.min(100,Math.max(0,status.downloaded/status.total*100)):null;}

// 只解析用于发布说明的基础块结构；HTML、图片和链接保持普通文本。
export function parseUpdateNotes(text) {
  const blocks=[];let paragraph=[],list=[],code=null;
  const flush=()=>{if(paragraph.length){blocks.push({type:'paragraph',text:paragraph.join(' ')});paragraph=[];}if(list.length){blocks.push({type:'list',items:list});list=[];}};
  for(const line of String(text||'').slice(0,256*1024).split(/\r?\n/)){
    if(line.startsWith('```')){flush();if(code!==null){blocks.push({type:'code',text:code.join('\n')});code=null;}else code=[];continue;}
    if(code!==null){code.push(line);continue;}
    const title=line.match(/^#{1,6}\s+(.+)$/),item=line.match(/^\s*(?:[-*]|\d+[.)])\s+(.+)$/);
    if(title){flush();blocks.push({type:'heading',text:title[1]});}
    else if(item){if(paragraph.length)flush();list.push(item[1]);}
    else if(!line.trim())flush();
    else {if(list.length)flush();paragraph.push(line.trim());}
  }
  flush();if(code!==null)blocks.push({type:'code',text:code.join('\n')});return blocks;
}
