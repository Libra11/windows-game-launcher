import test from 'node:test';
import assert from 'node:assert/strict';
import { searchMedia, personWorks, peopleIndexMessage } from '../src/lib/media-discovery.js';
import { createMediaNavigation } from '../src/lib/media-navigation.js';
import { createMediaPreview } from '../src/lib/media-preview.js';
import { createMediaSearchIndex, createMediaSearchIndexAsync, createMediaSearchRunner, searchMediaIndex, searchMediaIndexAsync, mediaItemContext } from '../src/lib/media-search-index.js';
import { queryMedia, mediaItemTitle, libraryRoots } from '../src/lib/media-query.js';

const items = [
  { id: 'movie', kind: 'movie', title: '星际穿越', year: '2014', watched: true, playable: true },
  { id: 'series', kind: 'series', recognized: true, title: '某剧', watched: false, playable: true },
  { id: 'season', kind: 'season', parentId: 'series', title: '第一季', watched: true },
  { id: 'record', kind: 'movie', title: '无资源电影', watched: true, playable: false },
];
const person = {
  id: 1,
  name: '同名人员',
  aliases: ['English Name'],
  credits: [
    { itemId: 'movie', roles: ['角色', 'Director'], scope: 'movie' },
    { itemId: 'series', roles: ['角色'], scope: 'tv' },
    { itemId: 'record', roles: ['Writer'], scope: 'movie' },
    { itemId: 'removed', roles: [], scope: 'movie' },
  ],
};
const data = {
  items,
  people: [person, { id: 2, name: person.name, credits: [{ itemId: 'series' }] }],
};

test('人员作品使用具体季的演职员关系和观看状态，不继承整剧关系', () => {
  const seasonal = {
    ...person,
    credits: [...person.credits, { itemId: 'season', scope: 'season', roles: ['本季角色'] }],
  };
  const result = personWorks(seasonal, items, { kind: 'series', watchedFilter: 'watched' });
  assert.deepEqual(
    result.map((v) => v.id),
    ['season']
  );
  assert.deepEqual(result[0].personCredit.roles, ['本季角色']);
});

test('聚合搜索区分片名与人员，别名与同名人员保持独立身份', () => {
  const result = searchMedia(data, '同名人员');
  assert.deepEqual(result.items, []);
  assert.deepEqual(
    result.people.map((value) => value.id),
    [1, 2]
  );
  assert.deepEqual(
    searchMedia(data, 'ＥＮＧＬＩＳＨ name').people.map((value) => value.id),
    [1]
  );
  assert.deepEqual(
    searchMedia(data, '2014').items.map((value) => value.id),
    ['movie']
  );
  assert.deepEqual(searchMedia(data, '  '), { items: [], people: [] });
});

function oldSearch(value, query) {
  const normalize = (v) => String(v || '').normalize('NFKC').trim().toLocaleLowerCase();
  const term = normalize(query), all = value.items || [];
  if (!term) return { items: [], people: [] };
  const found = libraryRoots(all.filter((item) => normalize(`${mediaItemTitle(item, all)} ${item.title} ${item.year || ''}`).includes(term)));
  const score = (v) => normalize(v) === term ? 0 : normalize(v).startsWith(term) ? 1 : 2;
  found.sort((a, b) => score(mediaItemTitle(a, all)) - score(mediaItemTitle(b, all)) ||
    mediaItemTitle(a, all).localeCompare(mediaItemTitle(b, all), 'zh-CN', { numeric: true }));
  const ids = new Set(all.map(item => item.id));
  const people = (value.people || []).filter(p => [p.name, ...(p.aliases || [])].some(n => normalize(n).includes(term)))
    .map(person => ({person, count:new Set((person.credits || []).filter(c => ids.has(c.itemId)).map(c => c.itemId)).size}))
    .filter(v => v.count > 0).sort((a,b) => score(a.person.name)-score(b.person.name) || b.count-a.count).map(v => v.person);
  return { items: found, people };
}

test('索引搜索和按关系定位作品保持旧结果与排序，包括同名和无效关联', () => {
  const extra = [
    { id:'child-movie', kind:'movie', parentId:'series', title:'某剧电影' },
    { id:'unknown-parent', kind:'movie', parentId:'removed', title:'某剧电影' },
    { id:'season2', kind:'season', parentId:'series', seasonNumber:2, title:'第2季', year:'2024', playable:true },
    { id:'episode', kind:'episode', parentId:'season2', seasonNumber:2, episodeNumber:3, title:'某剧番外' },
  ];
  const all = [...items, ...extra];
  const many = [...data.people, { id:3, name:'同名人员', aliases:['某剧'], credits:[{itemId:'season2'},{itemId:'movie'},{itemId:'movie'},{itemId:'child-movie'}] }];
  const value = { items: all, people: many };
  const index = createMediaSearchIndex(value);
  for (const query of ['某剧','同名','ＥＮＧＬＩＳＨ','2024','1','无资源','不存在'])
    assert.deepEqual(searchMediaIndex(index, query), oldSearch(value, query));
  const context = mediaItemContext(all);
  for (const p of many) for (const options of [{}, {kind:'series'}, {kind:'movie',onlyPlayable:true}, {watchedFilter:'watched'}, {sort:'za'}]) {
    const credits = new Map((p.credits || []).map(c => [c.itemId,c]));
    const old = queryMedia(all, {category: options.kind === 'movie' ? 'movies' : options.kind === 'series' ? 'series' : 'all', sort: options.sort || 'az', watchedFilter: options.watchedFilter || 'all', onlyPlayable: !!options.onlyPlayable})
      .filter(item => ['movie','season'].includes(item.kind) && credits.has(item.id))
      .map(item => ({...item, personCredit:credits.get(item.id)}));
    assert.deepEqual(personWorks(p, all, options, context), old);
  }
});

test('分段搜索结果一致，过期搜索可以取消', async () => {
  const many = Array.from({length: 4500}, (_, id) => ({ id, name:'同名人员', credits:[{itemId:'movie'}] }));
  const value = { items, people: many };
  const index = await createMediaSearchIndexAsync(value);
  assert.deepEqual(await searchMediaIndexAsync(index, '同名'), searchMediaIndex(index, '同名'));
  const runner = createMediaSearchRunner();
  const old = runner.search(value, '同名');
  const next = runner.search(value, '星际');
  assert.equal(await old, null);
  assert.deepEqual((await next).items.map(item => item.id), ['movie']);
  runner.dispose();
  assert.equal(await runner.search(value, '同名'), null);
});

test('人员作品包含无资源记录、排除已删除作品，整剧关系不推断到已看季', () => {
  assert.deepEqual(
    personWorks(person, items)
      .map((value) => value.id)
      .sort(),
    ['movie', 'record']
  );
  assert.deepEqual(
    personWorks(person, items, { watchedFilter: 'watched', onlyPlayable: true }).map(
      (value) => value.id
    ),
    ['movie']
  );
  assert.deepEqual(personWorks(person, items, { kind: 'series', watchedFilter: 'watched' }), []);
  assert.deepEqual(
    personWorks(person, items, { kind: 'movie' })
      .map((value) => value.id)
      .sort(),
    ['movie', 'record']
  );
  assert.deepEqual(
    personWorks(person, items).find((value) => value.id === 'movie').personCredit.roles,
    ['角色', 'Director']
  );
});

test('剧名和季名搜索均显示季，不归并为整剧', () => {
  assert.deepEqual(
    searchMedia(data, '第一季').items.map((value) => value.id),
    ['season']
  );
  assert.deepEqual(
    searchMedia(data, '某剧').items.map((value) => value.id),
    ['season']
  );
  assert.deepEqual(searchMedia(data, '已移除').items, []);
  assert.deepEqual(
    searchMedia(data, '无资源').items.map((item) => item.id),
    ['record']
  );
  assert.deepEqual(searchMedia({ items: [], people: [person] }, person.name).people, []);
});

test('索引尚未完成与获取失败会说明范围，完整索引不显示提示', () => {
  assert.match(peopleIndexMessage({ total: 10, completed: 7, failed: 2 }), /3 部.*2 部/);
  assert.equal(peopleIndexMessage({ total: 10, completed: 10, failed: 0 }), '');
});

test('浏览历史逐级返回，保留侧栏范围、搜索词和详情季集，不回滚新资料或播放状态', () => {
  const state = {
    page: 'library',
    category: 'unwatched',
    collectionId: 'collection',
    tagIds: ['tag'],
    selection: new Set(['movie']),
    data: { revision: 1 },
    playerStatus: { positionSeconds: 1 },
  };
  let ui = { scroll: 400, focus: 'media-movie' };
  const history = createMediaNavigation(state, {
    capture: () => ui,
    restore: (value) => {
      ui = value;
    },
  });
  history.push();
  Object.assign(state, { page: 'search', searchQuery: '演员', selection: new Set(), tagIds: [] });
  history.push();
  Object.assign(state, { page: 'person', person, personQuery: { onlyPlayable: true } });
  history.push();
  Object.assign(state, {
    page: 'detail',
    selectedId: 'series',
    detailSeasonId: 'season',
    detailEpisodeId: 'episode',
    detailResourceId: 'version',
  });
  history.push();
  state.page = 'person';
  state.person = { id: 99 };
  state.detailSeasonId = '';
  state.data = { revision: 2 };
  state.playerStatus = { positionSeconds: 50 };
  assert.equal(history.back(), true);
  assert.equal(state.detailSeasonId, 'season');
  assert.equal(state.detailEpisodeId, 'episode');
  assert.equal(state.detailResourceId, 'version');
  assert.equal(history.back(), true);
  assert.equal(state.person.id, 1);
  assert.equal(state.personQuery.onlyPlayable, true);
  assert.equal(history.back(), true);
  assert.equal(state.searchQuery, '演员');
  assert.equal(history.back(), true);
  assert.equal(state.category, 'unwatched');
  assert.equal(state.collectionId, 'collection');
  assert.deepEqual(state.tagIds, ['tag']);
  assert.deepEqual([...state.selection], ['movie']);
  assert.equal(ui.scroll, 400);
  assert.equal(state.data.revision, 2);
  assert.equal(state.playerStatus.positionSeconds, 50);
  assert.equal(history.back(), false);
});

test('切换侧栏分类后清空临时访问路径', () => {
  const state = { page: 'library', category: 'movies' };
  const history = createMediaNavigation(state);
  history.push();
  state.page = 'search';
  history.clear();
  state.page = 'library';
  state.category = 'series';
  assert.equal(history.length, 0);
  assert.equal(history.back(), false);
  assert.equal(state.category, 'series');
});

test('公开预览的人员搜索与详情演职员使用同一人员身份', async () => {
  const command = createMediaPreview();
  const data = await command('media_snapshot');
  const presentation = await command('media_presentation', { itemId: 'preview-movie-1' });
  const person = presentation.people[0];
  const result = searchMedia(data, person.name);
  assert.ok(result.people.some((value) => value.id === person.id));
  const works = personWorks(
    result.people.find((value) => value.id === person.id),
    data.items
  );
  assert.ok(works.some((value) => value.id === 'preview-movie-1'));
});


test('清理条目后浏览历史跳过已删除详情，保留原列表及其滚动状态', () => {
  const state={page:'library',selectedId:'',data:{revision:1}};
  let ui={scroll:800};
  const history=createMediaNavigation(state,{capture:()=>ui,restore:value=>{ui=value;}});
  history.push();state.page='detail';state.selectedId='old-series';history.push();
  state.selectedId='old-season';history.push();
  history.discardItems(new Set(['old-series','old-season']));
  assert.equal(history.length,1);assert.equal(history.back(),true);assert.equal(state.page,'library');assert.equal(state.selectedId,'');assert.equal(ui.scroll,800);
});
