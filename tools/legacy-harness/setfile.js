// Injecte un fichier dans un <input type=file> reel du renderer, via
// DOM.setFileInputFiles : Chromium declenche alors l'evenement "change" natif.
// usage: node setfile.js <index ap-file-input> <chemin absolu>
//   index 0 = video_library (journal / rejeu), 1 = streetview (GPX / KML)
const WebSocket = require('/home/tom/node_modules/ws');
const http = require('http');

const idx = parseInt(process.argv[2], 10);
const file = process.argv[3];

http.get('http://127.0.0.1:9222/json/list', (res) => {
  let d = '';
  res.on('data', (c) => (d += c));
  res.on('end', async () => {
    const page = JSON.parse(d).find((t) => t.type === 'page');
    const ws = new WebSocket(page.webSocketDebuggerUrl, { perMessageDeflate: false });
    let id = 0;
    const pending = new Map();
    ws.on('message', (m) => {
      const o = JSON.parse(m);
      if (o.id && pending.has(o.id)) { pending.get(o.id)(o); pending.delete(o.id); }
    });
    await new Promise((r) => ws.on('open', r));
    const send = (method, params) => new Promise((r) => {
      const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params: params || {} }));
    });
    await send('DOM.enable');
    const doc = await send('DOM.getDocument', { depth: -1, pierce: true });
    const q = await send('DOM.querySelectorAll', { nodeId: doc.result.root.nodeId, selector: 'ap-file-input input[type=file]' });
    const ids = q.result.nodeIds;
    console.log(`inputs trouves: ${ids.length}`);
    if (!ids[idx]) { console.log('ABSENT'); process.exit(1); }
    const r = await send('DOM.setFileInputFiles', { nodeId: ids[idx], files: [file] });
    console.log(r.error ? 'ERR ' + JSON.stringify(r.error) : `injecte ${file} -> input ${idx}`);
    setTimeout(() => process.exit(0), 300);
  });
});
