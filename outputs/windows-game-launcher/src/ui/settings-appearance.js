import { themePicker } from './theme-controls.js';
import { fontSettings } from './font-settings.js';
import { settingsSection } from './settings-components.js';

export function settingsAppearance(actions, saved) {
  const element=settingsSection('外观与字体','选择界面主题，按自己的阅读习惯排列字体。更改会立即生效。');
  const theme=themePicker();
  theme.element.addEventListener('click',event=>{if(event.target.closest('.theme-option'))saved('主题偏好已保存');});
  element.append(theme.element,fontSettings(actions,saved));
  return {element,dispose:theme.dispose};
}
