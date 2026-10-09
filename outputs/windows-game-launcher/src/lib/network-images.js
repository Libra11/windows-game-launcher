import { isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

let revision = 0;
export const failedArtwork = new Set();

export function imageSource(source, desktop = isTauri(), version = revision) {
  if (!desktop || !/^https?:\/\//i.test(source)) return source;
  return `http://youji-image.localhost/?url=${encodeURIComponent(source)}&v=${version}`;
}

export function setImageSource(image, source) {
  image.dataset.remoteSource = source;
  image.addEventListener('load', () => { image.hidden = false; });
  image.src = imageSource(source);
}

export function retryImages() {
  revision++;
  failedArtwork.clear();
  document.querySelectorAll('[data-artwork-key]').forEach(host => {
    if (!host.classList.contains('art-loaded')) host.dispatchEvent(new Event('network-image-retry'));
  });
  document.querySelectorAll('img[data-remote-source]').forEach(image => {
    if (!image.closest('[data-artwork-key]') && (!image.complete || image.naturalWidth === 0)) image.src = imageSource(image.dataset.remoteSource);
  });
}

export async function watchNetworkImages() {
  if (isTauri()) return listen('network-changed', retryImages);
  return () => {};
}
