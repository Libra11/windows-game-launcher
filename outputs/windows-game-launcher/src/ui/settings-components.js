import { el } from '../lib/dom.js';

export function settingsSection(title, description) {
  const root=el('section','settings-section');
  const header=el('header','settings-section-header');header.append(el('h2','',title),el('p','settings-description',description));root.append(header);return root;
}
export function settingsToggle(title, description, checked, change) {
  const row=el('label','settings-row');const text=el('div','settings-row-copy');
  text.append(el('strong','',title),el('span','',description));
  const input=el('input','settings-switch');input.type='checkbox';input.checked=checked;input.setAttribute('role','switch');input.setAttribute('aria-label',title);
  input.onchange=async()=>{
    const next=input.checked;input.disabled=true;
    try {await change(next);}catch{input.checked=!next;}finally{input.disabled=false;}
  };
  row.append(text,input);return row;
}
