import test from 'node:test';
import assert from 'node:assert/strict';
import { mediaSourceLinks } from '../src/lib/media-source-links.js';
import { detailModel, metadataSource, metadataSearchTitle } from '../src/lib/media-detail-model.js';

test('季和集的识别搜索使用剧名，独立豆瓣季保留作品名', () => {
  const series={id:'tv',kind:'series',title:'庆余年'};
  const season={id:'s',kind:'season',parentId:'tv',title:'第 2 季'};
  const episode={id:'e',kind:'episode',parentId:'s',title:'第一集'};
  assert.equal(metadataSearchTitle(season,[series,season]),'庆余年');
  assert.equal(metadataSearchTitle(episode,[series,season]),'庆余年');
  assert.equal(metadataSearchTitle({id:'d',kind:'season',title:'庆余年 第二季'},[]),'庆余年 第二季');
});

test('作品链接直达电影、季、集和豆瓣并去重，拒绝异常编号', () => {
  const links = mediaSourceLinks({ externalIds: [
    { provider:'tmdb', providerType:'season', externalId:'100:0' },
    { provider:'tmdb', providerType:'episode', externalId:'100:0:2' },
    { provider:'tmdb', providerType:'movie', externalId:'200' },
    { provider:'douban', providerType:'subject', externalId:'42' },
    { provider:'douban', providerType:'subject', externalId:'42' },
    { provider:'tmdb', providerType:'movie', externalId:'1/../../evil' },
    { provider:'douban', providerType:'subject', externalId:'42?query=1' },
  ], imdbId:'tt1234567' });
  assert.deepEqual(links.map(link=>link.href),[
    'https://www.themoviedb.org/tv/100/season/0',
    'https://www.themoviedb.org/tv/100/season/0/episode/2',
    'https://www.themoviedb.org/movie/200',
    'https://movie.douban.com/subject/42/',
    'https://www.imdb.com/title/tt1234567/',
  ]);
});

test('无分集的豆瓣季仍选中关联文件并保持独立续播位置', () => {
  const item = {id:'season', kind:'season', recognized:true, externalIds:[{provider:'douban',providerType:'subject',externalId:'42'}]};
  const resources = [{id:'a',itemIds:['season'],available:true,positionSeconds:30},
    {id:'b',itemIds:['season'],available:true,positionSeconds:90}];
  const model = detailModel({item,children:[],resources},[],{resourceId:'b'});
  assert.equal(model.recordItem,item);
  assert.equal(model.episode,undefined);
  assert.equal(model.resource.id,'b');
  assert.equal(model.resource.positionSeconds,90);
  assert.equal(model.versions.length,2);
  assert.equal(metadataSource(item),'豆瓣');
  assert.equal(metadataSource({...item,recognized:false}),'豆瓣导入');
});
