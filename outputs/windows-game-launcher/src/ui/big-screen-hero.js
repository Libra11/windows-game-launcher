import { el, button, icon, artwork, metadata, sourceName, sourceIcon } from '../lib/dom.js';
import { playedDate } from '../lib/library-query.js';
import { launchButton, installButton, favoriteButton, runtimeBadge, installationBadge, playtimeBadge } from './game-controls.js';
import { reveal } from './motion.js';
import './big-screen-hero.css';

export function showBigScreenGame(hero, backdrop, game, actions, animate = false) {
  const art = el('div', 'big-screen-scene'); artwork(art, game, true); backdrop.replaceChildren(art);
  const copy = el('div', 'big-screen-copy');
  const labels = el('div', 'big-screen-hero-labels');
  const platform = el('span', 'big-screen-platform');
  platform.append(icon(sourceIcon(game)), document.createTextNode(sourceName(game)));
  labels.append(el('span', 'big-screen-eyebrow', game.lastPlayed ? '继续你的冒险' : '你的下一场冒险'), platform);
  const title = el('h1', '', game.title); title.title = game.title;
  copy.append(labels, title, el('p', 'big-screen-description', metadata(game).description || '打开这个世界，开启属于你的下一段旅程。'));
  const context = el('div', 'big-screen-hero-context');
  if (game.lastPlayed) context.append(el('span', '', `上次游玩 · ${playedDate(game.lastPlayed)}`));
  context.append(playtimeBadge(game));
  if (context.childElementCount) copy.append(context);
  const status = el('div', 'big-screen-hero-status'); status.append(runtimeBadge(game), installationBadge(game)); copy.append(status);
  const buttons = el('div', 'big-screen-hero-actions');
  const details = button('详情与成就', 'big-screen-details', () => actions.select(game), 'trophy'); details.dataset.focusKey = 'details';
  const favorite = favoriteButton(game, actions, false, 'hero-favorite');
  favorite.className = 'big-screen-favorite favorite-button-wide'; favorite.replaceChildren(icon('star'));
  buttons.append(launchButton(game, actions, 'primary', 'launch'));
  if (game.source === 'epic') buttons.append(installButton(game,actions,'big-screen-details','install'));
  buttons.append(details, favorite); copy.append(buttons);

  const stage = el('div', 'big-screen-hero-stage');
  const poster = button('', 'big-screen-hero-poster', () => actions.select(game));
  poster.setAttribute('aria-label', `查看 ${game.title} 的详情与成就`); poster.dataset.focusKey = 'hero-cover';
  const cover = el('div', 'big-screen-hero-cover'); artwork(cover, game); poster.append(cover); stage.append(poster);
  hero.replaceChildren(copy, stage);
  if (animate) {
    reveal(copy, { distance:5, duration:220 });
    reveal(art, { distance:0, duration:400 });
    reveal(stage, { distance:7, duration:320 });
  }
}
