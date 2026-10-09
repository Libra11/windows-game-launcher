export const failedArtwork = new Set();

export function setImageSource(image, source) {
  image.dataset.remoteSource = source;
  image.addEventListener('load', () => { image.hidden = false; });
  image.src = source;
}

export function retryImages() {
  failedArtwork.clear();
  document.querySelectorAll('[data-artwork-key]').forEach(host => {
    const loading = [...host.children].some(node => node.tagName === 'IMG');
    if (!host.classList.contains('art-loaded') && !loading) host.dispatchEvent(new Event('network-image-retry'));
  });
  document.querySelectorAll('img[data-remote-source]').forEach(image => {
    if (!image.closest('[data-artwork-key]') && image.complete && image.naturalWidth === 0) {
      image.removeAttribute('src');
      image.hidden = false;
      image.src = image.dataset.remoteSource;
    }
  });
}

export async function watchNetworkImages() {
  window.addEventListener('online', retryImages);
  return () => window.removeEventListener('online', retryImages);
}
