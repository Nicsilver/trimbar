const HOST = 'io.github.nicsilver.trimbar';

// Chrome only injects content scripts into pages loaded after install or update, so tabs that were
// already open would ignore the trim until reloaded.
chrome.runtime.onInstalled.addListener(async () => {
  for (const tab of await chrome.tabs.query({})) {
    chrome.scripting.executeScript({ target: { tabId: tab.id }, files: ['content.js'] }).catch(() => {});
  }
});

async function fetchTargets() {
  try {
    const reply = await chrome.runtime.sendNativeMessage(HOST, {});
    const targets = reply?.targets ?? [];
    await chrome.storage.session.set({ targets });
    return targets;
  } catch {
    return null;
  }
}

chrome.runtime.onMessage.addListener((message, sender, respond) => {
  if (message !== 'targets') return;
  (async () => {
    const { targets: cached } = await chrome.storage.session.get('targets');
    if (!cached) {
      respond(await fetchTargets());
      return;
    }
    // Answer from the cache so the video doesn't sit in the dead rows while the host starts,
    // then correct it if the trims changed since.
    respond(cached);
    const fresh = await fetchTargets();
    if (fresh && JSON.stringify(fresh) !== JSON.stringify(cached)) {
      chrome.tabs.sendMessage(sender.tab.id, { targets: fresh }, { frameId: sender.frameId }).catch(() => {});
    }
  })();
  return true;
});
