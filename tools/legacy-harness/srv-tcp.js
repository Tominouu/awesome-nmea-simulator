// Serveur TCP pour tester le mode tcpclient du legacy.
// usage: node srv-tcp.js [port] [sortie] [duree_s] [ferme_a_ms]
const net = require('net');
const fs = require('fs');

const port = parseInt(process.argv[2] || '15001', 10);
const out = process.argv[3] || 'cap-tcpclient.log';
const dur = parseInt(process.argv[4] || '20', 10) * 1000;
const closeAt = process.argv[5] ? parseInt(process.argv[5], 10) : null;

const srv = net.createServer((sock) => {
  const id = srv._conns = (srv._conns || 0) + 1;
  const t0 = Date.now();
  let n = 0, bytes = 0, first = null, last = null;
  const gaps = [];
  const label = '# CONNEXION ' + id + ' depuis ' + sock.remoteAddress + ':' + sock.remotePort;
  console.error(label);
  fs.appendFileSync(out, label + '\n');

  sock.on('data', (b) => {
    n++; bytes += b.length;
    const dt = Date.now() - t0;
    if (first === null) first = dt;
    if (last !== null) gaps.push(dt - last);
    last = dt;
    fs.appendFileSync(out, '[' + dt + 'ms] ' + JSON.stringify(b.toString()) + '\n');
  });
  sock.on('error', (e) => fs.appendFileSync(out, '# SOCKET ' + id + ' erreur ' + e.message + '\n'));
  sock.on('end', () => fs.appendFileSync(out, '# FIN ' + id + ' cote client\n'));
  sock.on('close', (h) => fs.appendFileSync(out, '# CLOSE ' + id + ' hadError=' + h +
    ' datagrammes=' + n + ' octets=' + bytes + ' ecart_median=' +
    (gaps.length ? gaps.slice().sort((a, b) => a - b)[Math.floor(gaps.length / 2)] : 'n/a') + 'ms\n'));

  if (closeAt !== null) {
    setTimeout(() => {
      console.error('fermeture forcee de la connexion ' + id + ' a ' + closeAt + 'ms');
      fs.appendFileSync(out, '# SERVEUR ferme la connexion ' + id + ' a ' + closeAt + 'ms\n');
      sock.destroy();
    }, closeAt);
  }
});

srv.on('error', (e) => { console.error('ERR ' + e.message); process.exit(1); });
srv.listen(port, '127.0.0.1', () => console.error('TCP server listening on 127.0.0.1:' + port));

setTimeout(() => {
  fs.appendFileSync(out, '# RECAP port=' + port + ' connexions=' + (srv._conns || 0) +
    ' premier=' + Date.now() + '\n');
  srv.close();
  process.exit(0);
}, dur);
