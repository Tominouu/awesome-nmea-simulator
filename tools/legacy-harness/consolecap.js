// Capture le console du renderer (Runtime.consoleAPICalled + Log.entryAdded
// + exceptions) pendant N secondes. Les messages de la classe de transport
// vivent dans le renderer, donc absents de app.log.
// usage: node consolecap.js [duree_s] [sortie]
const WebSocket = require('/home/tom/node_modules/ws');
const http = require('http');
const fs = require('fs');

const dur = parseInt(process.argv[2] || '20', 10) * 1000;
const out = process.argv[3] || 'console.log';

http.get('http://127.0.0.1:9222/json/list', (res) => {
  let d = '';
  res.on('data', (c) => (d += c));
  res.on('end', () => {
    const page = JSON.parse(d).find((t) => t.type === 'page');
    const ws = new WebSocket(page.webSocketDebuggerUrl, { perMessageDeflate: false, maxPayload: 64 * 1024 * 1024 });
    let id = 0;
    const send = (method, params) => ws.send(JSON.stringify({ id: ++id, method, params: params || {} }));
    const fmt = (a) => (Array.isArray(a) ? a.map((x) => (x && x.value !== undefined ? x.value : x.description || x.type)).join(' ') : String(a));
    ws.on('open', () => {
      send('Runtime.enable');
      send('Log.enable');
      send('Runtime.evaluate', { expression: "console.log('=== consolecap armed ===')", returnByValue: true });
      setTimeout(() => { console.error('console capture closee'); process.exit(0); }, dur);
    });
    ws.on('message', (m) => {
      const o = JSON.parse(m);
      if (o.method === 'Runtime.consoleAPICalled') {
        const t = o.params.type;
        if (t === 'debug' || t === 'verbose') return;
        const line = '[' + o.params.timestamp.toFixed(3) + '] console.' + t + ' ' + fmt(o.params.args);
        fs.appendFileSync(out, line + '\n');
        console.error(line);
      } else if (o.method === 'Log.entryAdded') {
        const e = o.params.entry;
        const line = '[' + e.timestamp.toFixed(3) + '] log.' + e.level + ' ' + e.source + ' ' + e.text;
        fs.appendFileSync(out, line + '\n');
        console.error(line);
      } else if (o.method === 'Runtime.exceptionThrown') {
        const d0 = o.params.exceptionDetails;
        const line = '[' + d0.timestamp.toFixed(3) + '] EXCEPTION ' + (d0.exception && (d0.exception.description || d0.exception.value) || d0.text);
        fs.appendFileSync(out, line + '\n');
        console.error(line);
      }
    });
  });
}).on('error', (e) => { console.error('ERR ' + e.message); process.exit(1); });
