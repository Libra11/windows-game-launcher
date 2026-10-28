(() => {
  try {
    const clean = value => String(value || '').replace(/\s+/g, ' ').trim();
    const url = new URL(location.href);
    const text = document.body?.innerText || '';
    const restricted = url.hostname === 'sec.douban.com' || /检测到有异常请求|访问过于频繁|请求过于频繁/.test(text.slice(0, 2000)) || !!document.querySelector('form[action*="captcha"], input[name="captcha-solution"], #captcha_image');
    const login = /accounts\.douban\.com/.test(url.hostname) || !!document.querySelector('input[type="password"], iframe[src*="passport/login"]') || /请登录后重试/.test(text);
    const links = [...document.querySelectorAll('a[href]')];
    const ownCollect = links.find(a => {
      try { const u = new URL(a.href); return u.hostname === 'movie.douban.com' && /^\/people\/[^/]+\/collect\/?$/.test(u.pathname) && clean(a.textContent) === '看过'; }
      catch { return false; }
    });
    const collectUrl = ownCollect?.href || '';
    const currentAccount = url.pathname.match(/^\/people\/([^/]+)\/collect\/?$/)?.[1] || '';
    const mineAccount = collectUrl ? new URL(collectUrl).pathname.match(/^\/people\/([^/]+)\/collect\/?$/)?.[1] || '' : '';
    const accountNode = document.querySelector('#db-global-nav .nav-user-account .bn-more span, #db-global-nav .nav-user-account > a');
    const loggedIn = !!document.querySelector('#db-global-nav .nav-user-account') && !/登录|注册/.test(clean(accountNode?.textContent));
    const isList = /^\/people\/[^/]+\/collect\/?$/.test(url.pathname);
    const records = isList ? [...document.querySelectorAll('.grid-view .item, .list-view .item')].map(item => {
      const subjectLinks = [...item.querySelectorAll('a[href*="/subject/"]')];
      const a = subjectLinks.find(link => clean(link.textContent)) || subjectLinks[0];
      if (!a) return null;
      const subjectId = new URL(a.href).pathname.match(/^\/subject\/(\d+)\//)?.[1];
      if (!subjectId) return null;
      const stars = [...item.querySelectorAll('[class*="rating"]')].map(e => e.className.match(/rating([1-5])-t/)?.[1]).find(Boolean);
      const image = item.querySelector('.pic img, img');
      const intro = clean(item.querySelector('.intro')?.textContent);
      return {
        subjectId, title:clean(a.textContent) || clean(image?.alt), stars:stars ? Number(stars) : null,
        markedAt:clean(item.querySelector('.date')?.textContent).match(/\d{4}-\d{2}-\d{2}/)?.[0] || '',
        tags:clean(item.querySelector('.tags')?.textContent).replace(/^标签[:：]\s*/, '').split(/\s+/).filter(Boolean),
        comment:clean(item.querySelector('.comment')?.textContent),
        posterUrl:image?.getAttribute('data-origin') || image?.getAttribute('src') || '',
        year:intro.match(/(?:19|20)\d{2}/)?.[0] || '', intro
      };
    }).filter(Boolean) : [];
    const next = document.querySelector('.paginator .next a[href], a[rel="next"]');
    const countText = clean(document.querySelector('.subject-num')?.textContent);
    const total = Number(countText.match(/\/\s*([\d,]+)/)?.[1]?.replaceAll(',', '') || countText.match(/([\d,]+)\s*部/)?.[1]?.replaceAll(',', '') || 0);
    const empty = isList && records.length === 0 && !restricted && !login && /还没有|暂无|没有符合条件|0\s*部/.test(text);
    const subjectId = url.pathname.match(/^\/subject\/(\d+)\/?$/)?.[1] || '';
    const info = document.querySelector('#info')?.innerText || '';
    const imdbId = info.match(/(?:^|\n)\s*IMDb\s*[:：]\s*(tt\d+)\s*(?:\n|$)/i)?.[1] || '';
    const seasonNumber = info.match(/(?:^|\n)\s*季数\s*[:：]\s*(\d+)\s*(?:\n|$)/)?.[1];
    // 豆瓣现用 link-report-intra；折叠简介优先取完整版本，保留旧页面结构。
    const summaryRoots = ['#link-report-intra', '#link-report'];
    const summary = summaryRoots.flatMap(root => [
      `${root} [property="v:summary"].all`, `${root} .all [property="v:summary"]`,
    ]).map(selector => document.querySelector(selector)).find(node => clean(node?.textContent))
      || summaryRoots.map(root => document.querySelector(`${root} [property="v:summary"]`)).find(node => clean(node?.textContent));
    const detail = subjectId && info && document.querySelector('h1') ? {
      subjectId, imdbId,
      title:clean(document.querySelector('h1 [property="v:itemreviewed"]')?.textContent),
      seasonNumber:seasonNumber === undefined ? null : Number(seasonNumber),
      kind:/(?:^|\n)\s*首播\s*[:：]/.test(info) ? 'series' : /(?:^|\n)\s*上映日期\s*[:：]/.test(info) ? 'movie' : '',
      year:clean(document.querySelector('h1 .year')?.textContent).match(/\d{4}/)?.[0] || '',
      overview:clean(summary?.textContent),
      posterUrl:document.querySelector('#mainpic img')?.getAttribute('src') || '',
      genres:[...document.querySelectorAll('#info [property="v:genre"]')].map(node=>clean(node.textContent)).filter(Boolean),
    } : null;
    return { ready:document.readyState, href:url.href, isMine:url.pathname === '/mine', isList, loggedIn,
      accountKey:currentAccount || mineAccount, displayName:clean(accountNode?.textContent).replace(/的帐号$/, ''),
      collectHref:collectUrl, records, next:next?.href || '', total, empty, restricted, login,
      isSubject:!!subjectId, detail };
  } catch { return { error:'parse_error' }; }
})()
