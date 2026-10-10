import { el, button, icon, artwork, sourceName, sourceIcon, metadata } from '../lib/dom.js';
import { playedDate } from '../lib/library-query.js';
import { launchButton, installButton, runtimeBadge, installationBadge, playtimeBadge } from './game-controls.js';
import './library-feature.css';

export function libraryFeature(game, actions) {
  const info = metadata(game);
  const hero = el('section', 'library-feature');
  hero.setAttribute('aria-label', `${game.title}，推荐游戏`);
  const exhibit = el('div', 'library-feature-exhibit');
  const art = el('div', 'library-feature-art');
  artwork(art, game, true);
  const copy = el('div', 'library-feature-copy');
  const topline = el('div', 'library-feature-topline');
  const kicker = el('span', 'library-feature-kicker');
  kicker.append(el('span', 'library-feature-dot'), document.createTextNode(game.lastPlayed ? '继续游玩' : '库中精选'));
  const edition = el('span', 'library-feature-edition', game.favorite ? '珍藏之选' : '你的下一场冒险');
  topline.append(kicker, edition);

  const title = el('h2', 'library-feature-title', game.title);
  title.title = game.title;
  const description = el('p', 'library-feature-description', info.description || '打开熟悉的世界，开始下一段冒险。');
  copy.append(topline, title, el('span', 'library-feature-rule'), description);

  const stage = el('div', 'library-feature-stage');
  const poster = button('', 'library-feature-poster', () => actions.select(game));
  poster.setAttribute('aria-label', `查看 ${game.title} 的详情与成就`);
  poster.dataset.focusKey = `featured-poster-${game.id}`;
  const cover = el('div', 'library-feature-cover');
  artwork(cover, game);
  const open = el('span', 'library-feature-open'); open.append(icon('arrow'));
  poster.append(cover, open);
  stage.append(poster);
  exhibit.append(art, copy, stage);

  const information = el('div', 'library-feature-information');
  const platform = el('span', 'library-feature-platform');
  platform.append(icon(sourceIcon(game)), document.createTextNode(sourceName(game)));
  const context = el('div', 'library-feature-context');
  if (game.lastPlayed) {
    context.append(el('span', '', `上次游玩 · ${playedDate(game.lastPlayed)}`));
  } else {
    context.append(el('span', '', game.favorite ? '已加入你的收藏' : '已加入你的游戏库'));
  }
  context.append(playtimeBadge(game));
  information.append(platform, context);
  const status = el('div', 'library-feature-status');
  status.append(runtimeBadge(game), installationBadge(game));
  information.append(status);
  copy.append(information);
  const buttons = el('div', 'library-feature-actions');
  buttons.append(button('详情与成就', 'library-feature-details', () => actions.select(game), 'arrow'), launchButton(game, actions));
  if (game.source === 'epic') buttons.append(installButton(game,actions,'library-feature-install'));
  hero.append(exhibit, buttons);
  return hero;
}
