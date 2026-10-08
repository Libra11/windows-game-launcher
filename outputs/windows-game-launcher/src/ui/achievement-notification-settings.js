import { el } from '../lib/dom.js';
export async function achievementNotificationSettings(actions) {
  const options = await actions.run('get_achievement_overlay_options');
  const element = el('div', 'form');
  function select(label, values, value) {
    const wrapper = el('label', 'field');
    wrapper.append(el('span', '', label));
    const input = el('select');
    for (const [key, text] of values) {
      const option = el('option', '', text); option.value = key; input.append(option);
    }
    input.value = String(value); wrapper.append(input); element.append(wrapper);
    return input;
  }
  const position = select('成就弹层位置', [['bottom-right','右下角'],['bottom-left','左下角'],['top-right','右上角'],['top-left','左上角']], options.position);
  const duration = select('停留时间', [[3,'3 秒'],[5,'5 秒'],[8,'8 秒'],[10,'10 秒']], options.duration);
  const sound = select('成就提示音', [['chime','星光'],['soft','轻响'],['off','静音']], options.sound);
  const save = () => actions.run('save_achievement_overlay_options', { options: { position: position.value, duration: Number(duration.value), sound: sound.value } });
  for (const input of [position, duration, sound]) input.addEventListener('change', () => {
    save().catch(error => actions.toast(String(error), true));
  });
  return { element, save };
}
