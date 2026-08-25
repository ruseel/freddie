import type { IncomingEvent } from "./wire/IncomingEvent";

// Must match mercury's `--port` / `MERCURY_PORT`. The options page overrides it.
const DEFAULT_PORT = 3883;

let socket: WebSocket | null = null;

/**
 * Last URL sent on `socket`. Cleared when the socket drops, so a new mercury still
 * receives the front tab even if this side already sent that URL to the previous one.
 */
let lastSent: string | null = null;

async function port(): Promise<number> {
  const { port } = await chrome.storage.local.get({ port: DEFAULT_PORT });
  return typeof port === "number" ? port : DEFAULT_PORT;
}

/** Open a socket if needed and return it. Do not reread the module variable after awaiting. */
async function connect(): Promise<WebSocket> {
  if (
    socket !== null &&
    (socket.readyState === WebSocket.OPEN ||
      socket.readyState === WebSocket.CONNECTING)
  ) {
    return socket;
  }
  const ws = new WebSocket(`ws://127.0.0.1:${String(await port())}`);
  // Clear only this socket. A failed connect fires `error` then `close`; by then a later
  // tab event may already have replaced `socket`.
  const forget = (): void => {
    if (socket !== ws) return;
    socket = null;
    lastSent = null;
  };
  ws.addEventListener("close", forget);
  ws.addEventListener("error", forget);
  socket = ws;
  return ws;
}

/** Send `url` to mercury. A send that cannot go out is dropped; the next tab event supersedes it. */
async function pushUrl(url: string | undefined): Promise<void> {
  if (url === undefined || url === "") return;
  if (url === lastSent) return;
  const ws = await connect();
  lastSent = url;
  const frame: IncomingEvent = { kind: "IncomingEvent.Tab", value: { url } };
  const payload = JSON.stringify(frame);
  if (ws.readyState === WebSocket.OPEN) {
    ws.send(payload);
  } else {
    ws.addEventListener(
      "open",
      () => {
        ws.send(payload);
      },
      { once: true },
    );
  }
}

chrome.tabs.onActivated.addListener(({ tabId }) => {
  void chrome.tabs.get(tabId).then((tab) => pushUrl(tab.url));
});

// `onUpdated` fires per changed tab; keep the active one, and only when a URL arrived.
chrome.tabs.onUpdated.addListener((_tabId, info, tab) => {
  if (info.url !== undefined && tab.active) void pushUrl(info.url);
});

// App-switch and Chrome-window switch change the front tab with no tab event.
// `WINDOW_ID_NONE` is Chrome losing focus.
chrome.windows.onFocusChanged.addListener((windowId) => {
  if (windowId === chrome.windows.WINDOW_ID_NONE) return;
  void chrome.tabs
    .query({ active: true, windowId })
    .then(([tab]) => pushUrl(tab?.url));
});

/**
 * Same-document navigation in the front tab (`pushState`, `replaceState`, fragment).
 * `onUpdated` only covers document loads. `frameId === 0` is the top frame.
 */
const onSameDocument = ({
  tabId,
  frameId,
  url,
}: chrome.webNavigation.WebNavigationTransitionCallbackDetails): void => {
  if (frameId !== 0) return;
  void chrome.tabs.get(tabId).then((tab) => {
    if (tab.active) void pushUrl(url);
  });
};

chrome.webNavigation.onHistoryStateUpdated.addListener(onSameDocument);
chrome.webNavigation.onReferenceFragmentUpdated.addListener(onSameDocument);
