// Chrome forces `transform: none` on the fullscreen element but leaves the `scale` property
// alone, so a uniform scale anchored at the top lifts the whole player, controls included,
// above the trimmed strip. Sites like YouTube fullscreen the root element, which has no
// backdrop, so the root's own background is painted black instead.

const style = document.createElement('style');
let targets = [];

function apply() {
  if (!document.fullscreenElement) {
    style.textContent = '';
    return;
  }
  const x = screenX + outerWidth / 2;
  const y = screenY + outerHeight / 2;
  const t = targets.find((t) => x >= t.left && x < t.left + t.width && y >= t.top && y < t.top + t.height);
  style.textContent = t
    ? `:fullscreen { scale: ${(t.height - t.trim) / t.height} !important; transform-origin: top center !important; }
       :root:fullscreen { background: black !important; }`
    : '';
  if (!style.isConnected) {
    document.documentElement.append(style);
  }
}

function update(next) {
  if (Array.isArray(next)) {
    targets = next;
    apply();
  }
}

document.addEventListener(
  'fullscreenchange',
  () => {
    apply();
    if (document.fullscreenElement) {
      chrome.runtime.sendMessage('targets').then(update, () => {});
    }
  },
  true,
);

chrome.runtime.onMessage.addListener((message) => update(message?.targets));
