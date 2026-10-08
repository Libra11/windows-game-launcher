import { el, icon } from '../lib/dom.js';
import './custom-select.css';

let sequence=0;

// 自定义列表使用顶层弹出层，避免被设置页或对话框的滚动区域裁切。
export function customSelect(options, initialValue, label, className='') {
  const values=options.map(([value,text])=>[String(value),text]);
  const root=el('div','custom-select '+className);
  const trigger=el('button','custom-select-trigger');trigger.type='button';
  const caption=el('span');trigger.append(caption,icon('chevron'));
  const menu=el('div','custom-select-menu');menu.popover='auto';menu.id='select-menu-'+sequence++;
  menu.setAttribute('role','listbox');menu.setAttribute('aria-label',label);
  trigger.setAttribute('role','combobox');trigger.setAttribute('aria-label',label);
  trigger.setAttribute('aria-haspopup','listbox');trigger.setAttribute('aria-controls',menu.id);trigger.setAttribute('aria-expanded','false');
  let value=String(initialValue),highlight=0;
  if(!values.some(([id])=>id===value))value=values[0]?.[0]||'';
  const items=values.map(([id,text],index)=>{
    const option=el('button','custom-select-option');option.type='button';option.tabIndex=-1;
    option.id=menu.id+'-'+index;option.setAttribute('role','option');
    option.append(el('span','',text),icon('check'));
    option.onclick=()=>choose(index);
    option.onpointermove=()=>{highlight=index;paint();};
    menu.append(option);return option;
  });
  root.append(trigger,menu);
  function paint() {
    caption.textContent=values.find(([id])=>id===value)?.[1]||'';
    items.forEach((item,index)=>{
      const selected=values[index][0]===value;
      item.setAttribute('aria-selected',String(selected));item.classList.toggle('highlighted',index===highlight);
    });
    if(menu.matches(':popover-open'))trigger.setAttribute('aria-activedescendant',items[highlight]?.id||'');
  }
  function close(focus=false) {
    if(menu.matches(':popover-open'))menu.hidePopover();
    trigger.setAttribute('aria-expanded','false');trigger.removeAttribute('aria-activedescendant');
    if(focus)trigger.focus({preventScroll:true});
  }
  function open() {
    if(trigger.disabled||!values.length)return;
    highlight=Math.max(0,values.findIndex(([id])=>id===value));
    const rect=trigger.getBoundingClientRect(),below=innerHeight-rect.bottom,above=rect.top;
    const upward=below<180&&above>below;
    const width=Math.min(Math.max(rect.width,190),innerWidth-24);
    menu.style.width=width+'px';menu.style.maxHeight=Math.max(80,Math.min(320,(upward?above:below)-16))+'px';
    menu.style.left=Math.max(12,Math.min(rect.left,innerWidth-width-12))+'px';
    menu.showPopover();menu.style.top=(upward?rect.top-menu.getBoundingClientRect().height-6:rect.bottom+6)+'px';
    trigger.setAttribute('aria-expanded','true');paint();items[highlight]?.scrollIntoView({block:'nearest'});
  }
  function choose(index) {
    const next=values[index][0],changed=next!==value;value=next;close(true);paint();
    if(changed)root.dispatchEvent(new Event('change',{bubbles:true}));
  }
  trigger.onclick=()=>menu.matches(':popover-open')?close():open();
  trigger.onkeydown=event=>{
    const opened=menu.matches(':popover-open');
    if(['ArrowDown','ArrowUp','Home','End'].includes(event.key)){
      event.preventDefault();if(!opened){open();return;}
      highlight=event.key==='Home'?0:event.key==='End'?values.length-1:Math.min(values.length-1,Math.max(0,highlight+(event.key==='ArrowDown'?1:-1)));
      paint();items[highlight]?.scrollIntoView({block:'nearest'});
    }else if(event.key==='Enter'||event.key===' '){event.preventDefault();opened?choose(highlight):open();}
    else if(event.key==='Escape'){event.preventDefault();close(true);}
    else if(event.key==='Tab')close();
  };
  root.addEventListener('focusout',event=>{if(!root.contains(event.relatedTarget))close();});
  menu.addEventListener('toggle',event=>{
    trigger.setAttribute('aria-expanded',String(event.newState==='open'));
    if(event.newState==='closed')trigger.removeAttribute('aria-activedescendant');
  });
  Object.defineProperties(root,{
    value:{get:()=>value,set:next=>{value=String(next);paint();}},
    disabled:{get:()=>trigger.disabled,set:disabled=>{trigger.disabled=!!disabled;if(disabled)close();}},
  });
  root.focus=options=>trigger.focus(options);
  trigger.addEventListener('focus',()=>{if(root.dataset.focusKey)trigger.dataset.focusKey=root.dataset.focusKey;});
  paint();return root;
}
