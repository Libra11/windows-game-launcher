import { invoke, isTauri, convertFileSrc } from '@tauri-apps/api/core';
import { localArtworkKey, mediaArtworkSource } from './media-artwork-model.js';
import { createFrameQueue } from './media-frame-queue.js';

const pending = new Map();
const localUrls = new Map();
const queueFrame = createFrameQueue();
function needsArtwork(image) {
  if (!image.isConnected) return false;
  const box = image.getBoundingClientRect?.();
  return !box || (box.bottom >= -250 && box.top <= (globalThis.innerHeight || Infinity) + 250
    && box.right >= -250 && box.left <= (globalThis.innerWidth || Infinity) + 250);
}
const artworkKey = (item, role) => !mediaArtworkSource(item, role) && localArtworkKey(item, role)
  ? `frame:${item.id}:${localArtworkKey(item, role)}`
  : `${item.id}:${role}:${item.personId || ''}:${!!item.recognized}:${mediaArtworkSource(item, role)}:${localArtworkKey(item, role)}:${item.artworkPreviewToken || ''}`;
export function cachedMediaArtwork(item, role = 'poster') {
  return item?.id ? localUrls.get(artworkKey(item, role)) || '' : '';
}
let observer;
const targets = new WeakMap();
const observed = new Set();
let cleanupQueued = false;
const retries = new Set();
let retryTimer;
function retryLater(image) {
  const target = targets.get(image);
  if (!target || !localArtworkKey(target.item, target.role)) return;
  target.retryAt = Date.now() + 60_000;
  retries.add(image);
  scheduleRetries();
}
function scheduleRetries() {
  if (retryTimer) return;
  retryTimer = setTimeout(() => {
    retryTimer = null;
    for (const previous of retries) {
      const target = targets.get(previous);
      if (!previous.isConnected || !target?.retryAt) retries.delete(previous);
      else if (Date.now() >= target.retryAt) {
        retries.delete(previous);
        retryMediaArtwork(previous);
      }
    }
    if (retries.size) scheduleRetries();
  }, 60_000);
  retryTimer.unref?.();
}
export function retryMediaArtwork(image) {
  const target = targets.get(image);
  if (!target || !target.retryAt || Date.now() < target.retryAt || target.loading) return;
  target.retryAt = 0;
  target.resolved = false;
  if (observer) { observed.add(image); observer.observe(image); }
  else queueMicrotask(() => load(image, target.item, target.role));
}
function load(image, item, role) {
  const source = mediaArtworkSource(item, role);
  const localKey = localArtworkKey(item, role);
  if ((!source && !localKey) || !image.isConnected) return;
  const key = artworkKey(item, role);
  const requestTarget = targets.get(image);
  if (requestTarget?.loading) return;
  if (requestTarget) requestTarget.loading = true;
  if (!pending.has(key)) {
    const images = new Set([image]);
    const input = { itemId: item.id, role, personId: item.personId, localArtworkKey: localKey || undefined };
    if (item.artworkPreviewToken) input.previewToken = item.artworkPreviewToken;
    const frame = !source && !!localKey;
    const promise = invoke('media_artwork', { input: { ...input, cacheOnly: frame } })
      .then(result => !frame || result?.path ? result : queueFrame(
        () => invoke('media_artwork', { input }),
        () => [...images].some(needsArtwork)
      )).catch(() => null);
    const request = { promise, images };
    pending.set(key, request);
    promise.finally(() => { if (pending.get(key) === request) pending.delete(key); });
  }
  pending.get(key).images.add(image);
  pending.get(key).promise.then((result) => {
    const target = targets.get(image);
    if (target !== requestTarget) return;
    if (target) { target.resolved = true; target.loading = false; }
    if (result?.path) {
      const local = convertFileSrc(result.path);
      if (target) target.local = local;
      localUrls.delete(key);
      localUrls.set(key, local);
      if (localUrls.size > 500) localUrls.delete(localUrls.keys().next().value);
      // 已经显示的网络图片不再切换来源，避免替换 src 时闪烁。
      if (image.isConnected && image.src !== local && !(image.complete && image.naturalWidth > 0)) {
        image.style.opacity = '';
        image.src = local;
      }
    } else if (target?.rawFailed || !source) { target?.fail?.(); retryLater(image); }
  });
}
export function loadMediaArtwork(image, item, role = 'poster') {
  if (!isTauri() || !item?.id) return;
  const original = image.onerror;
  const target = {
    item,
    role,
    resolved: false,
    rawFailed: false,
    local: cachedMediaArtwork(item, role),
    triedRawFallback: false,
    fail: () => original?.call(image, new Event('error')),
  };
  targets.set(image, target);
  image.onerror = () => {
    if (target.local && image.src === target.local) {
      localUrls.delete(artworkKey(item, role));
      pending.delete(artworkKey(item, role));
      target.local = '';
      image.removeAttribute?.('src');
      const source = mediaArtworkSource(item, role);
      if (source && !target.triedRawFallback) {
        target.triedRawFallback = true;
        image.src = source;
      } else {
        target.fail();
        // 缓存被回收或损坏后重新向后端解析，允许原地恢复截图。
        retryLater(image);
      }
    } else if (target.local) {
      image.style.opacity = '';
      image.src = target.local;
    } else if (target.resolved) target.fail();
    else {
      target.rawFailed = true;
      image.style.opacity = '0';
    }
  };
  if (typeof IntersectionObserver === 'undefined') {
    queueMicrotask(() => load(image, item, role));
    return;
  }
  observer ||= new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (!entry.isIntersecting) continue;
        observer.unobserve(entry.target);
        observed.delete(entry.target);
        const data = targets.get(entry.target);
        if (data) load(entry.target, data.item, data.role);
      }
    },
    { rootMargin: '250px' }
  );
  observed.add(image);
  observer.observe(image);
  if (!cleanupQueued) {
    cleanupQueued = true;
    queueMicrotask(() => {
      cleanupQueued = false;
      for (const previous of observed) {
        if (!previous.isConnected) {
          observer.unobserve(previous);
          observed.delete(previous);
        }
      }
    });
  }
}
