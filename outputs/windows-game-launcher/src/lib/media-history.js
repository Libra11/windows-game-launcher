// 日期由后端按本机时区生成，与日历使用同一口径。
export function mediaHistoryRecords(statistics, date = '') {
  const records = statistics.records || [];
  if (!date) return records;
  return records
    .filter((record) => record.activityDates.includes(date))
    .map((record) => ({ ...record, activityAt: date }));
}

export function mediaHistorySessions(statistics, date = '') {
  return (statistics.sessions || []).filter((session) => !date || session.activityDate === date);
}
