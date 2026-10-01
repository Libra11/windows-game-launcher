const vectors = { left: [-1, 0], right: [1, 0], up: [0, -1], down: [0, 1] };

export function gridNeighbor(current, count, columns, direction) {
  if (current < 0 || current >= count || columns < 1) return -1;
  if (direction === 'left') return current % columns ? current - 1 : -1;
  if (direction === 'right') return (current + 1) % columns && current + 1 < count ? current + 1 : -1;
  if (direction === 'up') return current >= columns ? current - columns : -1;
  const nextRow = (Math.floor(current / columns) + 1) * columns;
  return nextRow < count ? Math.min(current + columns, count - 1) : -1;
}

export function nextFocus(rectangles, current, direction) {
  const [dx, dy] = vectors[direction];
  const origin = rectangles[current];
  if (!origin) return rectangles.length ? 0 : -1;
  let best = -1, score = Infinity;
  rectangles.forEach((rect, index) => {
    if (index === current) return;
    const x = rect.x - origin.x, y = rect.y - origin.y;
    const forward = x * dx + y * dy;
    if (forward <= 1) return;
    const side = Math.abs(x * dy - y * dx);
    const distance = forward + side * 3;
    if (distance < score) { score = distance; best = index; }
  });
  return best;
}

export function gamepadActions(pad) {
  if (!pad || pad.mapping !== 'standard') return [];
  const pressed = index => pad.buttons[index]?.pressed;
  const x = pad.axes[0] || 0, y = pad.axes[1] || 0;
  const actions = [];
  if (pressed(14) || (x < -.5 && Math.abs(x) >= Math.abs(y))) actions.push('left');
  if (pressed(15) || (x > .5 && Math.abs(x) >= Math.abs(y))) actions.push('right');
  if (pressed(12) || (y < -.5 && Math.abs(y) > Math.abs(x))) actions.push('up');
  if (pressed(13) || (y > .5 && Math.abs(y) > Math.abs(x))) actions.push('down');
  for (const [index, action] of [[0, 'confirm'], [1, 'back'], [4, 'previous'], [5, 'next']]) {
    if (pressed(index)) actions.push(action);
  }
  return actions;
}

// 导航允许按住重复；确认和返回只在按下瞬间触发。
export function repeatedActions(actions, held, now) {
  const fired = [];
  const directions = Object.keys(vectors);
  for (const action of actions) {
    if (!held.has(action)) { fired.push(action); held.set(action, now + 360); }
    else if (directions.includes(action) && now >= held.get(action)) { fired.push(action); held.set(action, now + 150); }
  }
  for (const action of held.keys()) if (!actions.includes(action)) held.delete(action);
  return fired;
}
