import { el, button } from '../../lib/dom.js';
import { safeImageUrl } from '../../lib/media-query.js';
import { presentationKey } from '../../lib/media-presentation.js';
import { cachedMediaArtwork, loadMediaArtwork } from '../../lib/media-artwork.js';
import { personPortrait } from './discovery.js';
import './presentation.css';

export function titleTreatment(item, presentation, tag = 'h1') {
  const heading = el(tag, 'media-art-title');
  heading.append(el('span', '', item.title));
  const url = safeImageUrl(presentation.logoUrl);
  if (url) {
    const image = el('img');
    image.alt = '';
    image.setAttribute('aria-hidden', 'true');
    image.decoding = 'async';
    image.onload = () => heading.classList.add('has-logo');
    image.onerror = () => {
      heading.classList.remove('has-logo');
      image.remove();
    };
    heading.append(image);
    const artwork = { ...item, logoUrl: url };
    loadMediaArtwork(image, artwork, 'logo');
    image.src = cachedMediaArtwork(artwork, 'logo') || url;
    if (image.complete && image.naturalWidth > 0) heading.classList.add('has-logo');
  }
  return heading;
}

export function creditsSection(controller, item) {
  if (!presentationKey(item)) return null;
  const data = controller.presentation(item);
  const root = el('section', 'media-credits');
  root.setAttribute('aria-label', '演职员');
  const heading = el('div', 'media-section-heading');
  heading.append(
    el('h2', '', '演职员'),
    el('span', 'media-credits-source', item.kind === 'season' ? 'TMDB · 本季' : 'TMDB · 整部作品')
  );
  root.append(heading);
  if (!data.people?.length) {
    const message = el('div', 'media-credits-status');
    message.append(
      el(
        'span',
        '',
        data.status === 'error'
          ? '演职员资料暂未获取'
          : data.status === 'ready'
            ? '暂无演职员资料'
            : '正在获取演职员…'
      )
    );
    if (data.status === 'error')
      message.append(
        button(
          '重新获取',
          'media-text-button',
          () => controller.loadPresentation(item, true),
          'refresh'
        )
      );
    root.append(message);
    return root;
  }
  const rail = el('div', 'media-credits-rail');
  rail.dataset.mediaRail = 'credits';
  rail.tabIndex = 0;
  rail.setAttribute('role', 'list');
  rail.setAttribute('aria-label', '主要演员与主创');
  for (const person of data.people) {
    const card = el('article');
    card.setAttribute('role', 'listitem');
    const open = button('', 'media-credit media-credit-button', () =>
      controller.openPerson(person, item)
    );
    open.dataset.focusKey = `media-credit-${person.id}`;
    open.setAttribute('aria-label', `查看 ${person.name} 的库内作品`);
    const portrait = personPortrait(person, item.id);
    const name = el('strong', '', person.name);
    name.title = person.name;
    const role = el('small', '', person.role || (person.department === 'cast' ? '演员' : '主创'));
    role.title = role.textContent;
    open.append(portrait, name, role);
    card.append(open);
    rail.append(card);
  }
  root.append(rail);
  return root;
}
