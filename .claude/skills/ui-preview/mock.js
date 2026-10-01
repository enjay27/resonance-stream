// Stand-in for window.__TAURI__ so the ui runs in a plain browser.
// Every invoke is recorded in window.__calls as [cmd, args] (args arrive as a
// JS Map from serde_wasm_bindgen -- Tauri's IPC flattens Maps, so does
// window.__callLog()). Add a case when a view needs data from a command.
window.__calls = [];
window.__callLog = () => window.__calls.map(([c, a]) =>
  c + JSON.stringify(a ?? null, (_k, v) => (v instanceof Map ? Object.fromEntries(v) : v)));
window.__mock = {
  load_config: () => window.__MOCK_CONFIG__,
  get_chat_history: () => [],
  get_system_history: () => [],
  get_network_interfaces: () => [{ name: 'Ethernet', ip: '192.168.0.2' }],
  grow_window: () => ({ x: 100, y: 100, width: 800, height: 600 }),
};
window.__TAURI__ = {
  core: {
    invoke: async (cmd, args) => {
      window.__calls.push([cmd, args]);
      const f = window.__mock[cmd];
      return f ? f(args) : null;
    },
  },
  event: { listen: async () => () => {} },
};
