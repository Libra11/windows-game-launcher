export function parseEpisodeNumbers(value, episodes) {
  const text = String(value || '').trim();
  if (!text) return [];
  const allowed = new Set(episodes.map(row => Number(row.episodeNumber)));
  const result = [];
  for (const part of text.replace(/，/g, ',').split(/[\s,]+/)) {
    const match = part.match(/^(\d+)(?:[-~～—](\d+))?$/);
    if (!match) throw Error('集号格式：1、1,2 或 1-2');
    const first = Number(match[1]), last = Number(match[2] ?? match[1]);
    if (last < first || last - first >= allowed.size) throw Error('分集范围无效');
    for (let n = first; n <= last; n++) {
      if (!allowed.has(n)) throw Error(`所选季没有第 ${n} 集`);
      if (result.includes(n)) throw Error('集号不能重复');
      result.push(n);
    }
  }
  return result;
}
export function suggestedEpisodes(file, episodes, offset = 0, overrideSeason = false) {
  if (file.disabled || !((file.canSuggest ?? !file.warning) || (overrideSeason && file.canOverrideSeason)) || !Number.isInteger(offset)) return [];
  const known = new Set(episodes.map(row => Number(row.episodeNumber)));
  const numbers = (file.parsedEpisodes || []).map(n => n + offset);
  return numbers.length && numbers.every(n => known.has(n)) ? numbers : [];
}
