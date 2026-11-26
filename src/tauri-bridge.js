(function initTauriBridge() {
  const tauri = window.__TAURI__ || {};
  const core = tauri.core || null;
  const eventApi = tauri.event || null;
  const windowApi = tauri.window || null;

  const listeners = new Map();
  const knownEvents = [
    'settings-updated',
    'sync-settings',
    'displays-updated',
    'display-config',
    'media-update',
    'lyrics-update',
    'phase-sync',
    'flyout-opened',
    'flyout-closed',
    'audio-frame',
    'audio-level',
  ];

  function emitLocal(channel, payload) {
    const handlers = listeners.get(channel);
    if (!handlers || handlers.size === 0) return;
    const evt = { sender: ipcRenderer };
    handlers.forEach((fn) => {
      try {
        fn(evt, payload);
      } catch (err) {
        console.error(`[tauri-bridge] listener error on "${channel}":`, err);
      }
    });
  }

  let readyResolve;
  const listenersReady = new Promise((resolve) => {
    readyResolve = resolve;
  });

  if (eventApi && typeof eventApi.listen === 'function') {
    Promise.all(
      knownEvents.map((eventName) =>
        eventApi
          .listen(eventName, (event) => {
            const payload = event ? event.payload : undefined;
            if (eventName === 'audio-frame') {
              emitLocal('audio-frame', payload);
              emitLocal('audio-level', payload);
            } else {
              emitLocal(eventName, payload);
            }
          })
          .catch((err) => {
            console.error(`[tauri-bridge] failed to listen to "${eventName}":`, err);
          })
      )
    ).finally(() => {
      readyResolve();
    });
  } else {
    readyResolve();
  }

  function invokeCommand(cmd, args) {
    if (!core || typeof core.invoke !== 'function') {
      return Promise.reject(new Error(`Tauri core.invoke unavailable for command "${cmd}"`));
    }
    return core.invoke(cmd, args || {}).catch((err) => {
      console.error(`[tauri-bridge] invoke("${cmd}") failed:`, err);
    });
  }

  const ipcRenderer = {
    on(channel, fn) {
      if (typeof fn !== 'function') return ipcRenderer;
      if (!listeners.has(channel)) {
        listeners.set(channel, new Set());
      }
      listeners.get(channel).add(fn);
      return ipcRenderer;
    },

    once(channel, fn) {
      if (typeof fn !== 'function') return ipcRenderer;
      const wrapper = (evt, ...args) => {
        ipcRenderer.removeListener(channel, wrapper);
        fn(evt, ...args);
      };
      return ipcRenderer.on(channel, wrapper);
    },

    removeListener(channel, fn) {
      const set = listeners.get(channel);
      if (set) {
        set.delete(fn);
      }
      return ipcRenderer;
    },

    off(channel, fn) {
      return ipcRenderer.removeListener(channel, fn);
    },

    send(channel, ...args) {
      switch (channel) {
        case 'overlay-ready':
          listenersReady.then(() => invokeCommand('overlay_ready'));
          break;
        case 'fullscreen-ready':
          listenersReady.then(() => invokeCommand('fullscreen_ready'));
          break;
        case 'settings-ready':
          listenersReady.then(() => invokeCommand('settings_ready'));
          break;
        case 'update-settings':
          invokeCommand('update_settings', { patch: args[0] || {} });
          break;
        case 'toggle-fullscreen-lyrics': {
          const raw = args[0];
          const open =
            typeof raw === 'boolean'
              ? raw
              : raw && typeof raw.open === 'boolean'
                ? raw.open
                : null;
          invokeCommand('toggle_fullscreen_lyrics', { open });
          break;
        }
        case 'fullscreen-hotkey':
          invokeCommand('fullscreen_hotkey');
          break;
        case 'set-ignore-mouse-events': {
          const raw = args[0];
          const ignore =
            typeof raw === 'object' && raw !== null ? Boolean(raw.ignore) : Boolean(raw);
          invokeCommand('set_ignore_cursor_events', { ignore });
          break;
        }
        case 'media-control': {
          const action = String(args[0] || '').toLowerCase().trim();
          if (action === 'play-pause' || action === 'playpause' || action === 'toggle') {
            invokeCommand('play_pause');
          } else if (action === 'next' || action === 'skip-next') {
            invokeCommand('next_track');
          } else if (action === 'previous' || action === 'prev' || action === 'skip-previous') {
            invokeCommand('prev_track');
          } else {
            invokeCommand('media_control', { action });
          }
          break;
        }
        case 'seek-media':
          invokeCommand('seek_to', { seconds: Number(args[0]) || 0 });
          break;
        case 'hide-settings':
          invokeCommand('hide_settings');
          break;
        case 'open-settings':
        case 'open-flyout':
          invokeCommand('open_settings');
          break;
        case 'minimize-window':
          invokeCommand('minimize_fullscreen');
          break;
        case 'toggle-fullscreen-window':
          invokeCommand('toggle_fullscreen_mode');
          break;
        case 'quit-app':
          invokeCommand('quit_app');
          break;
        default:
          console.warn(`[tauri-bridge] unhandled ipcRenderer.send("${channel}")`);
          break;
      }
    },
  };

  window.LumnTauriBridge = {
    ipcRenderer,
    invoke: invokeCommand,
    getCurrentWindow:
      windowApi && typeof windowApi.getCurrentWindow === 'function'
        ? () => windowApi.getCurrentWindow()
        : () => null,
  };

  if (typeof window.require !== 'function') {
    window.require = function tauriRequireShim(moduleName) {
      if (moduleName === 'electron') {
        return { ipcRenderer };
      }
      throw new Error(`Module "${moduleName}" is not available in Winh Tauri`);
    };
  }
})();
