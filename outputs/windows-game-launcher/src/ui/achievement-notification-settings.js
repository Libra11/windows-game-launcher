import { el } from '../lib/dom.js';
import { customSelect } from './custom-select.js';
export async function achievementNotificationSettings(actions, initial, saved=()=>{}) {
  const options = initial || await actions.run('get_achievement_overlay_options');
  const element = el('div', 'form');
  function select(label, values, value) {
    const wrapper = el('label', 'field');
    wrapper.append(el('span', '', label));
    const input = customSelect(values,value,label);wrapper.append(input); element.append(wrapper);
    return input;
  }
  const position = select('成就弹层位置', [['bottom-right','右下角'],['bottom-left','左下角'],['top-right','右上角'],['top-left','左上角']], options.position);
  const duration = select('停留时间', Array.from({length:8},(_,index)=>[index+3,(index+3)+' 秒']), options.duration);
  const sound = select('成就提示音', [['chime','星光'],['soft','轻响'],['off','静音']], options.sound);
  let pending=Promise.resolve();
  const save = () => {
    const options={position:position.value,duration:Number(duration.value),sound:sound.value};
    const task=pending.then(()=>actions.run('save_achievement_overlay_options',{options}));
    pending=task.catch(()=>{});return task.then(result=>{saved('成就提示已保存');return result;});
  };
  for (const input of [position, duration, sound]) input.addEventListener('change', () => {
    save().catch(error => actions.toast(String(error), true));
  });
  return { element, save };
}
