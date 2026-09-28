// Minimal Chrome DevTools Protocol driver
const WebSocket = require('/home/tom/node_modules/ws');
const http = require('http');

function getPage() {
  return new Promise((resolve, reject) => {
    http.get('http://127.0.0.1:9222/json/list', (res) => {
      let d = '';
      res.on('data', (c) => (d += c));
      res.on('end', () => {
        const list = JSON.parse(d);
        const page = list.find((t) => t.type === 'page');
        page ? resolve(page.webSocketDebuggerUrl) : reject(new Error('no page'));
      });
    }).on('error', reject);
  });
}

async function main() {
  const expr = process.argv[2];
  const url = await getPage();
  const ws = new WebSocket(url, { perMessageDeflate: false, maxPayload: 256 * 1024 * 1024 });
  let id = 0;
  const pending = new Map();
  ws.on('message', (m) => {
    const msg = JSON.parse(m);
    if (msg.id && pending.has(msg.id)) {
      pending.get(msg.id)(msg);
      pending.delete(msg.id);
    }
  });
  await new Promise((r) => ws.on('open', r));
  const send = (method, params) =>
    new Promise((res) => {
      const i = ++id;
      pending.set(i, res);
      ws.send(JSON.stringify({ id: i, method, params: params || {} }));
    });
  const r = await send('Runtime.evaluate', {
    expression: expr,
    awaitPromise: true,
    returnByValue: true,
    allowUnsafeEvalBlackboxing: true,
  });
  if (r.result && r.result.exceptionDetails) {
    console.log('EXCEPTION: ' + JSON.stringify(r.result.exceptionDetails, null, 1));
  } else {
    const v = r.result.result.value;
    console.log(typeof v === 'string' ? v : JSON.stringify(v, null, 1));
  }
  ws.close();
  process.exit(0);
}
main().catch((e) => {
  console.error('ERR', e.message);
  process.exit(1);
});
