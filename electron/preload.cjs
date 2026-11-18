const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('hlidskjalfDesktop', {
  setup: () => ipcRenderer.invoke('hlidskjalf:setup'),
  startup: (enabled) => ipcRenderer.invoke('hlidskjalf:startup', enabled),
  configureClipboard: (enabled) => ipcRenderer.invoke('hlidskjalf:clipboard-enabled', enabled),
  configureAudio: (enabled) => ipcRenderer.invoke('hlidskjalf:audio-enabled', enabled),
  onAudio: (callback) => ipcRenderer.on('hlidskjalf:audio', (_event, data) => callback(data)),
  locale: (language) => ipcRenderer.invoke('hlidskjalf:locale', language),
  createIdentity: () => ipcRenderer.invoke('hlidskjalf:create-identity'),
  choosePeerCert: () => ipcRenderer.invoke('hlidskjalf:choose-peer-cert'),
  pairById: (id) => ipcRenderer.invoke('hlidskjalf:pair-by-id', id),
  connect: (address) => ipcRenderer.invoke('hlidskjalf:connect', address),
  disconnect: () => ipcRenderer.send('hlidskjalf:disconnect'),
  input: (event) => ipcRenderer.send('hlidskjalf:input', event),
  onStatus: (callback) => ipcRenderer.on('hlidskjalf:session', (_event, status) => callback(status)),
  displays: () => ipcRenderer.invoke('hlidskjalf:displays'),
  configureEdge: (config) => ipcRenderer.invoke('hlidskjalf:edge-config', config),
  configureInput: (config) => ipcRenderer.invoke('hlidskjalf:input-settings', config),
  returnFromEdge: () => ipcRenderer.send('hlidskjalf:edge-return'),
  onEdgeState: (callback) => ipcRenderer.on('hlidskjalf:edge-state', (_event, active) => callback(active)),
  onInputError: (callback) => ipcRenderer.on('hlidskjalf:input-error', (_event, code) => callback(code)),
});
