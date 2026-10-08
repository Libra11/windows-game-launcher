import { el, button } from '../lib/dom.js';
import { field } from './form-fields.js';
import { customSelect } from './custom-select.js';
import { defaultFontFamilies, getFontFamilies, setFontFamilies, serializeFontFamilies, parseFontFamilies } from '../lib/font-family.js';
import './font-settings.css';

const choices=[['','添加常用字体'],['Segoe UI','Segoe UI'],['Microsoft YaHei','微软雅黑'],['Microsoft YaHei UI','微软雅黑 UI'],['SimSun','宋体'],['SimHei','黑体'],['KaiTi','楷体'],['Arial','Arial'],['Tahoma','Tahoma'],['Consolas','Consolas'],['PingFang SC','苹方'],['system-ui','系统 UI 字体'],['sans-serif','无衬线字体'],['serif','衬线字体'],['monospace','等宽字体']];

export function fontSettings(actions, saved) {
  const root=el('section','font-settings');let fonts=getFontFamilies();
  root.append(el('h3','','字体与回退顺序'),el('p','settings-description','优先使用排在前面的字体，缺少字形时依次回退。填写本机已安装字体的名称。'));
  const list=el('div','font-family-list');
  const chooser=customSelect(choices,'','添加常用字体');
  const custom=field('自定义字体','', '例如 Noto Sans SC，也可以粘贴多个字体');
  const additions=el('div','font-additions');
  const add=button('添加字体','secondary',()=>{
    try {const entries=parseFontFamilies(custom.input.value);if(!entries.length)throw new Error('请填写字体名称。');apply([...fonts,...entries]);custom.input.value='';}
    catch(error){actions.toast(String(error),true);}
  },'plus');
  additions.append(chooser,custom.wrapper,add);
  const editor=el('label','field font-css-field');editor.append(el('span','','CSS 字体序列'));
  const input=el('textarea','font-css-input');input.rows=2;input.spellcheck=false;input.setAttribute('aria-label','CSS 字体序列');editor.append(input);
  const editorTools=el('div','settings-tools');
  editorTools.append(button('应用序列','secondary',()=>{try{apply(parseFontFamilies(input.value));}catch(error){actions.toast(String(error),true);}}),button('恢复默认','settings-link',()=>apply(defaultFontFamilies),'refresh'));
  const preview=el('div','font-preview');
  preview.append(el('span','settings-overline','字体预览'),el('p','font-preview-title','让热爱，有迹可循。'),el('p','font-preview-alphabet','Aa Bb Cc · 0123456789 · 游戏与成就'));
  const css=el('code','font-preview-css');preview.append(css);
  function apply(next) {fonts=setFontFamilies(next);paint();saved('字体偏好已保存');}
  function move(index,delta) {const next=[...fonts],target=index+delta;[next[index],next[target]]=[next[target],next[index]];apply(next);}
  function paint() {
    list.replaceChildren();
    fonts.forEach((name,index)=>{
      const row=el('div','font-family-row'),text=el('div','font-family-copy');
      const title=el('strong','',name);title.style.fontFamily=serializeFontFamilies([name,...defaultFontFamilies]);
      text.append(title,el('small','',index===0?'优先字体':'回退顺序 '+(index+1)));
      const controls=el('div','font-family-actions');
      const up=button('','font-order-button font-up',()=>move(index,-1),'back');up.disabled=index===0;up.title='前移';up.setAttribute('aria-label','前移 '+name);
      const down=button('','font-order-button font-down',()=>move(index,1),'arrow');down.disabled=index===fonts.length-1;down.title='后移';down.setAttribute('aria-label','后移 '+name);
      const remove=button('','font-order-button',()=>apply(fonts.filter((_,position)=>position!==index)),'close');remove.disabled=fonts.length===1;remove.title='移除';remove.setAttribute('aria-label','从回退列表移除 '+name);
      controls.append(up,down,remove);row.append(el('span','font-family-index',String(index+1).padStart(2,'0')),text,controls);list.append(row);
    });
    input.value=serializeFontFamilies(fonts);css.textContent='font-family: '+input.value+';';
  }
  chooser.addEventListener('change',()=>{if(chooser.value){apply([...fonts,chooser.value]);chooser.value='';}});
  custom.input.addEventListener('keydown',event=>{if(event.key==='Enter'){event.preventDefault();add.click();}});
  root.append(list,additions,editor,editorTools,preview);paint();return root;
}
