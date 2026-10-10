import { el, button, icon } from '../lib/dom.js';
import { settingsAppearance } from './settings-appearance.js';
import { settingsConnections } from './settings-connections.js';
import { settingsNotifications } from './settings-notifications.js';
import { settingsGeneral } from './settings-general.js';
import { settingsNetwork } from './settings-network.js';
import { settingsData } from './settings-data.js';
import './settings.css';

const categories=[
  ['appearance','外观与字体','sun','主题与字体回退'],
  ['connections','游戏库与账号','library','账号连接与导入'],
  ['notifications','成就提示','trophy','位置、声音与通知'],
  ['general','后台与运行','settings','启动与退出行为'],
  ['network','网络与代理','network','连接方式与代理设置'],
  ['data','数据与迁移','folder','备份、恢复与路径重定位'],
];
let sequence=0;

export function createSettingsPage(actions) {
  const element=el('div','settings-page'),header=el('header','settings-page-header');
  const copy=el('div');copy.append(el('span','settings-overline','偏好与连接'),el('h1','','设置'),el('p','settings-description','让游迹更适合你的习惯。'));
  const status=el('p','settings-save-status','外观与开关自动保存');status.setAttribute('role','status');
  header.append(copy,status);element.append(header);
  const layout=el('div','settings-layout'),navigation=el('nav','settings-navigation'),content=el('div','settings-content');
  navigation.setAttribute('role','tablist');navigation.setAttribute('aria-label','设置分类');
  layout.append(navigation,content);element.append(layout);
  const id='settings-'+sequence++;let active='appearance',disposed=false,parts=[],savedSettings,queue=Promise.resolve();
  function saved(message='设置已保存'){if(!disposed)status.textContent=message;}
  const tabs=categories.map(([key,label,glyph,description])=>{
    const tab=button('','settings-category',()=>show(key,true));tab.setAttribute('role','tab');tab.id=id+'-tab-'+key;tab.setAttribute('aria-controls',id+'-'+key);
    const text=el('span');text.append(el('strong','',label));if(description)text.append(el('small','',description));tab.append(icon(glyph),text);navigation.append(tab);return tab;
  });
  function show(key,scroll=false) {
    active=key;
    tabs.forEach((tab,index)=>{const selected=categories[index][0]===key;tab.classList.toggle('active',selected);tab.setAttribute('aria-selected',String(selected));});
    parts.forEach((part,index)=>{part.element.hidden=categories[index][0]!==key;});
    if(scroll)window.scrollTo({top:0});
  }
  function save(patch) {
    // 串行合并已保存值，避免快速切换不同开关时覆盖其他偏好。
    const task=queue.then(async()=>{
      const next={...savedSettings,...patch};await actions.run('save_settings',next);savedSettings=next;saved();return next;
    });
    queue=task.catch(()=>{});return task;
  }
  async function load() {
    content.replaceChildren(el('p','settings-loading','正在读取设置…'));
    try {
      const [settings,options,network]=await Promise.all([actions.run('get_settings'),actions.run('get_achievement_overlay_options'),actions.run('get_network_settings')]);
      if(disposed)return;savedSettings={...settings};
      const appearance=settingsAppearance(actions,saved);
      const connections=settingsConnections(settings,actions,save);
      const networkPart=settingsNetwork(network,actions,saved);
      const notifications=await settingsNotifications(settings,actions,save,options,saved);
      const general=settingsGeneral(settings,actions,save);
      const built=[appearance,connections,notifications,general,networkPart,settingsData(actions)];
      if(disposed){built.forEach(part=>part.dispose());return;}
      parts=built;
      parts.forEach((part,index)=>{
        part.element.id=id+'-'+categories[index][0];part.element.setAttribute('role','tabpanel');part.element.setAttribute('aria-labelledby',tabs[index].id);
      });
      content.replaceChildren(...parts.map(part=>part.element));show(active);
    }catch(error){
      if(disposed)return;
      content.replaceChildren(el('p','settings-description','设置暂时无法读取：'+String(error)),button('重新读取','secondary',load,'refresh'));
    }
  }
  show(active);load();
  return {element,openCategory(key){if(categories.some(category=>category[0]===key))show(key);},dispose(){
    disposed=true;parts.forEach(part=>part.dispose());
    element.querySelectorAll('.custom-select-menu:popover-open').forEach(menu=>menu.hidePopover());
  }};
}
