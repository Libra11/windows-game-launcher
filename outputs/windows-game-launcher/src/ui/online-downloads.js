import { el, button, icon } from '../lib/dom.js';
import { resourceQrCode } from '../lib/online-detail.js';
import './online-downloads.css';

export function onlineDownloads(info, {open,copy,refresh,busy,selection}) {
  const root=el('section','online-resources');root.setAttribute('aria-label','下载地址和网盘二维码');
  const heading=el('div','online-resources-heading');
  const title=el('div','online-section-title');title.append(icon('download'),el('h2','','下载资源'));
  const links=info.links || [];
  if(links.length)title.append(el('span','online-resource-count',`${links.length} 个网盘`));
  const tools=el('div','online-resource-tools');
  const reload=button('刷新','secondary',refresh,'refresh');reload.setAttribute('aria-label','刷新下载信息');reload.dataset.focusKey='online-download-refresh';reload.disabled=busy;
  tools.append(reload);
  if(info.sourceUrl)tools.append(button('原站下载页','secondary',()=>open(info.sourceUrl,true),'network'));
  heading.append(title,tools);root.append(heading);
  if(info.error)root.append(el('p','online-resource-error',info.error));
  const passwords=el('div','online-resource-passwords');
  for(const [label,value] of [['文件密码',info.filePassword],['解压密码',info.archivePassword]]) {
    if(!value)continue;
    const field=el('div','online-resource-password');field.append(el('span','',label),el('code','',value));
    const control=button('复制','online-resource-copy',()=>copy(value,label));control.setAttribute('aria-label',`复制${label}`);
    field.append(control);passwords.append(field);
  }
  if(passwords.childElementCount)root.append(passwords);
  if(links.length) {
    const body=el('div','online-download-body');
    const providers=el('div','online-providers');providers.setAttribute('role','group');providers.setAttribute('aria-label','选择网盘');
    const selected=el('div','online-resource-selected');
    let active=Math.max(0,links.findIndex(link=>link.url===selection.url));
    const qrCache=new Map();const controls=[];
    function show(index) {
      active=index;const link=links[index];selection.url=link.url;
      controls.forEach((control,i)=>{control.classList.toggle('active',i===index);control.setAttribute('aria-pressed',String(i===index));});
      selected.replaceChildren();
      const content=el('div','online-resource-content');
      content.append(el('h3','',link.label),el('p','online-resource-caption','打开分享链接，或使用手机扫码。'));
      const address=el('p','online-resource-url',link.url);address.title=link.url;content.append(address);
      if(link.code){
        const code=el('div','online-resource-code');code.append(el('span','','提取码'),el('code','',link.code));
        code.append(button('复制提取码','online-resource-copy',()=>copy(link.code,'提取码')));content.append(code);
      }
      const actions=el('div','online-resource-actions');
      const visit=button(`打开${link.label}`,'primary',()=>open(link.url,true),'network');visit.dataset.focusKey='online-cloud-open';
      actions.append(visit,button('复制链接','secondary',()=>copy(link.url,'分享链接')));content.append(actions);
      const scan=el('div','online-resource-scan');
      try {
        if(!qrCache.has(link.url))qrCache.set(link.url,resourceQrCode(link.url));
        const frame=el('div','online-resource-qr');
        const source=qrCache.get(link.url);const markup=decodeURIComponent(source.slice(source.indexOf(',')+1));
        const svg=new DOMParser().parseFromString(markup,'image/svg+xml').documentElement;
        svg.setAttribute('role','img');svg.setAttribute('aria-label',`${link.label}分享地址二维码`);
        frame.append(document.importNode(svg,true));scan.append(frame,el('span','','扫码打开网盘'));
      } catch {scan.append(el('span','','二维码暂不可用，可直接打开或复制链接。'));}
      selected.append(content,scan);
    }
    links.forEach((link,index)=>{
      const control=button(link.label,'online-provider',()=>show(index),'network');control.dataset.focusKey=`online-provider-${index}`;
      const mark=el('span','online-provider-mark');mark.append(icon('check'));control.append(mark);controls.push(control);providers.append(control);
      control.addEventListener('keydown',event=>{
        if(!['ArrowDown','ArrowUp','ArrowRight','ArrowLeft'].includes(event.key))return;
        event.preventDefault();const step=['ArrowDown','ArrowRight'].includes(event.key)?1:-1;
        const next=(index+step+links.length)%links.length;show(next);controls[next].focus();
      });
    });
    show(active);body.append(providers,selected);root.append(body);
  } else if(!info.error)root.append(el('p','online-resource-caption','暂未读取到网盘链接，可在原站下载页查看。'));
  if(info.notes?.length) {
    const notes=el('div','online-resource-notes');
    for(const note of info.notes)notes.append(el('p','',note));root.append(notes);
  }
  return root;
}
