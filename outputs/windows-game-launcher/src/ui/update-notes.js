import { el } from '../lib/dom.js';
import { parseUpdateNotes } from '../lib/app-update.js';
function inline(host,text){
  for(const part of text.split(/(\*\*[^*]+\*\*|`[^`]+`)/g)){
    if(part.startsWith('**')&&part.endsWith('**'))host.append(el('strong','',part.slice(2,-2)));
    else if(part.startsWith('`')&&part.endsWith('`'))host.append(el('code','',part.slice(1,-1)));
    else host.append(document.createTextNode(part));
  }
}
export function renderUpdateNotes(text){
  const root=el('div','app-update-notes');
  for(const block of parseUpdateNotes(text)){
    if(block.type==='list'){const list=el('ul');for(const text of block.items){const item=el('li');inline(item,text);list.append(item);}root.append(list);}
    else {const node=el(block.type==='heading'?'h4':block.type==='code'?'pre':'p');block.type==='code'?node.textContent=block.text:inline(node,block.text);root.append(node);}
  }
  if(!root.children.length)root.append(el('p','','此版本未提供更新说明。'));return root;
}
