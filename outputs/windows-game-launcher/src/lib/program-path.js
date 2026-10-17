export function programDropPoint(position, scale = 1) {
  if (!position || !Number.isFinite(position.x) || !Number.isFinite(position.y) || !Number.isFinite(scale) || scale <= 0) return null;
  return { x:position.x / scale, y:position.y / scale };
}

export function createProgramPathResolver({ resolve, apply, failed }) {
  let version = 0, pending = Promise.resolve(), failure;
  function invalidate() { version++; failure = null; pending = Promise.resolve(); }
  function select(path) {
    const current = ++version;
    failure = null;
    pending = Promise.resolve().then(() => resolve(path)).then(value => {
      if (current === version) apply(value);
    }, error => {
      if (current === version) { failure = error; failed(error); }
    });
    return pending;
  }
  async function ready() {
    let task;
    do { task = pending; await task; } while (task !== pending);
    if (failure) throw failure;
  }
  return { select, invalidate, ready };
}
