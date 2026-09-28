// Recepteur UDP unicast (mode udpclient du legacy) : 127.0.0.1:15000
// usage: node cap-udpclient.js [port] [sortie] [duree_s]
const dgram = require('dgram');
const fs = require('fs');

const port = parseInt(process.argv[2] || '15000', 10);
const out = process.argv[3] || 'cap-udpclient.log';
const dur = parseInt(process.argv[4] || '20', 10) * 1000;

const s = dgram.createSocket('udp4');
let n = 0, bytes = 0, first = null, last = null;
const t0 = Date.now();
const gaps = [];

s.on('message', (b, rinfo) => {
  n++; bytes += b.length;
  const dt = Date.now() - t0;
  if (first === null) first = dt;
  if (last !== null) gaps.push(dt - last);
  last = dt;
  fs.appendFileSync(out, '[' + dt + 'ms] from ' + rinfo.address + ':' + rinfo.port +
    ' len=' + b.length + ' ' + JSON.stringify(b.toString()) + '\n');
});
s.on('error', (e) => { console.error('ERR ' + e.message); process.exit(1); });
s.bind(port, '127.0.0.1', () => console.error('UDP receiver listening on 127.0.0.1:' + port));

setTimeout(() => {
  s.close();
  fs.appendFileSync(out, '# RECAP port=' + port + ' datagrammes=' + n + ' octets=' + bytes +
    ' premier=' + first + 'ms dernier=' + last + 'ms ecart_median=' +
    (gaps.length ? gaps.slice().sort((a, b) => a - b)[Math.floor(gaps.length / 2)] : 'n/a') + 'ms\n');
  console.error('UDP: ' + n + ' datagrammes, ' + bytes + ' octets');
  process.exit(0);
}, dur);
