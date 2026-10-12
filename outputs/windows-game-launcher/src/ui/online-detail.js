import { el, button, icon } from '../lib/dom.js';
import { setImageSource } from '../lib/network-images.js';
import { createOnlineDetailController } from '../lib/online-detail.js';
import { onlineGamePresentation, onlineGameType } from '../lib/online-game-presentation.js';
import { onlineDownloads } from './online-downloads.js';
import './online-detail.css';

export function createOnlineDetailPage({command,preview,toast,back}) {
  const root=el('div','online-detail');
  const navigation=el('div','online-detail-navigation');
  const returnButton=button('返回搜索结果','detail-back',()=>{controller.cancel();back();},'back');
  returnButton.dataset.focusKey='online-detail-back';
  navigation.append(returnButton,el('span','online-detail-source',preview?'Gamer520 · 示例详情':'Gamer520'));
  const content=el('div','online-detail-content');root.append(navigation,content);
  const controller=createOnlineDetailController(command,update);
  const selection={article:'',url:''};
  let pendingFocus='';
  async function open(url,resource=false) {
    try {if(preview)window.open(url,'_blank','noopener,noreferrer');else await command(resource?'open_gamer520_resource':'open_gamer520_page',{url});}
    catch(error){toast(String(error),true);}
  }
  async function copy(value,label) {
    try {await navigator.clipboard.writeText(value);toast(label+'已复制');}
    catch {toast('无法写入剪贴板，请选中内容手动复制',true);}
  }
  function image(source,alt='',lazy=true) {
    const node=el('img');node.alt=alt;node.decoding='async';node.loading=lazy?'lazy':'eager';node.referrerPolicy='no-referrer';
    node.onerror=()=>{node.hidden=true;};setImageSource(node,source);return node;
  }
  function summary(item,state) {
    const data=state.data;
    const presentation=onlineGamePresentation(item.title);
    const section=el('section','online-detail-summary');
    const cover=el('div','online-detail-cover');const mark=el('span','online-detail-cover-mark');mark.append(icon('game'));cover.append(mark);
    const source=item.cover || state.item?.image;
    if(source){const img=image(source,'',false);img.onload=()=>cover.classList.add('loaded');cover.append(img);}
    const copy=el('div','online-detail-copy');
    const meta=el('div','online-detail-meta');meta.append(el('span','online-detail-platform',onlineGameType({...state.item,...item})));
    copy.append(meta,el('h1','',presentation.name));
    if(presentation.edition)copy.append(el('p','online-detail-version',presentation.edition));
    const actions=el('div','online-detail-actions');
    const jump=button('下载资源','primary',()=>{
      const resources=content.querySelector('.online-resources');resources?.scrollIntoView({block:'start'});
      resources?.querySelector('.online-provider.active, button')?.focus({preventScroll:true});
    },'download');jump.disabled=!data;
    actions.append(jump,button('查看原站','secondary',()=>open(item.url),'network'));copy.append(actions);
    section.append(cover,copy);return section;
  }
  function article(data) {
    const section=el('section','online-detail-reading');
    const heading=el('div','online-reading-heading');heading.append(el('h2','','游戏资料'),el('span','','来源 · Gamer520'));section.append(heading);
    const body=el('article','online-detail-article');body.setAttribute('aria-label','游戏介绍和截图');
    for(const block of data.blocks) {
      if(block.kind==='image') {
        if(block.image===data.cover)continue;
        const figure=el('figure');figure.append(image(block.image,'游戏介绍配图'));body.append(figure);
      } else body.append(el(block.kind==='heading'?'h3':'p','',block.text));
    }
    section.append(body);return section;
  }
  function update(state) {
    if(document.hasFocus()&&root.contains(document.activeElement))pendingFocus=document.activeElement.dataset.focusKey || '';
    const item=state.data || state.item;content.replaceChildren();if(!item)return;
    if(selection.article!==item.url){selection.article=item.url;selection.url='';}
    content.append(summary(item,state));
    if(state.loading)content.append(el('p','online-detail-status','正在读取游戏详情与下载信息…'));
    if(state.error){const error=el('div','online-resource-error');error.setAttribute('role','alert');error.append(el('p','',state.error),button('重试','secondary',()=>controller.load(),'refresh'));content.append(error);}
    if(state.data){
      content.append(onlineDownloads(state.data.downloads,{open,copy,selection,busy:state.loading,refresh:()=>controller.load()}),article(state.data));
    }
    if(!state.loading&&pendingFocus&&root.isConnected&&!root.closest('[hidden]')&&document.hasFocus()
      &&(root.contains(document.activeElement)||document.activeElement===document.body)) {
      [...root.querySelectorAll('[data-focus-key]')].find(node=>node.dataset.focusKey===pendingFocus&&!node.disabled)?.focus({preventScroll:true});pendingFocus='';
    }
  }
  return {element:root,load:controller.load,cancel:controller.cancel};
}
