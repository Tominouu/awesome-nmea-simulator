// Client TCP qui attend que le port ouvre, puis capture.
// usage: node cap-tcp-wait.js <port> <sortie> <duree_s> [attente_max_s]
const net = require('net');
const fs = require('fs');

const port = parseInt(process.argv[2] || '10110', 10);
const out = process.argv[3] || 'out.log';
const dur = parseInt(process.argv[4] || '18', 10) * 1000;
const maxWait = parseInt(process.argv[5] || '40', 10) * 1000;
const t0 = Date.now();

let s = null, tStart = null;

function connect() {
  s = net.createConnection({ host: '127.0.0.1', port }, () => {
    console.error('connecte a ' + port + ' apres ' + (Date.now() - t0) + 'ms');
    tStart = Date.now();
  });
  s.on('data', (b) => {
    if (tStart === null) tStart = Date.now();
    fs.appendFileSync(out, '[' + (Date.now() - tStart) + 'ms] ' + JSON.stringify(b.toString()) + '\n');
  });
  // 'error' est toujours suivi de 'close' : une seule reprise, programmee
  // dans 'close', sinon deux connexions paralleles ecrivent dans la sortie.
  s.on('error', () => s.destroy());
  s.on('close', () => {
    if (Date.now() - t0 < maxWait) setTimeout(connect, 500);
    else if (tStart === null) { console.error('abandon: port ' + port + ' jamais ouvert'); process.exit(1); }
  });
}
connect();

setTimeout(() => {
  if (s) s.end();
  process.exit(fs.existsSync(out) ? 0 : 1);
}, dur);
