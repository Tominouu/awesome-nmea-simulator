// Deux recepteurs UDP multicast sur 239.255.42.99:15002 via lo.
// But : prouver la diffusion a plusieurs recepteurs, et qu'un depart d'un
// recepteur n'interrompt pas l'autre (le multicast n'a pas de controle de flux).
// usage: node cap-mcast.js [groupe] [port] [sortie] [duree_s]
const dgram = require('dgram');
const fs = require('fs');

const group = process.argv[2] || '239.255.42.99';
const port = parseInt(process.argv[3] || '15002', 10);
const out = process.argv[4] || 'cap-mcast.log';
const dur = parseInt(process.argv[5] || '20', 10) * 1000;
const IFACE = '127.0.0.1';
const t0 = Date.now();

const stats = [];
const socks = [];

for (const id of [1, 2]) {
  const s = dgram.createSocket({ type: 'udp4', reuseAddr: true });
  s.on('error', (e) => { console.error('ERR R' + id + ' ' + e.message); });
  s.on('message', (b, rinfo) => {
    const st = stats[id - 1];
    st.n++; st.bytes += b.length;
    const dt = Date.now() - t0;
    if (st.first === null) st.first = dt;
    if (st.last !== null) st.gaps.push(dt - st.last);
    st.last = dt;
    fs.appendFileSync(out, '[' + dt + 'ms] R' + id + ' len=' + b.length + ' from ' +
      rinfo.address + ':' + rinfo.port + ' ' + JSON.stringify(b.toString()) + '\n');
  });
  s.bind({ address: group, port }, () => {
    try {
      s.addMembership(group, IFACE);
      console.error('R' + id + ' joint ' + group + ':' + port + ' via ' + IFACE);
    } catch (e) {
      console.error('R' + id + ' addMembership echoue: ' + e.message);
    }
  });
  socks.push(s);
  stats.push({ n: 0, bytes: 0, first: null, last: null, gaps: [] });
}

// A mi-capture R1 se retire ; on verifie que R2 continue de recevoir.
const retireAt = Math.floor(dur / 2);
setTimeout(() => {
  console.error('--- R1 se retire a ' + retireAt + 'ms ---');
  fs.appendFileSync(out, '# R1 SE RETIRE a ' + retireAt + 'ms (recepteur multicast absent)\n');
  socks[0].close();
}, retireAt);

setTimeout(() => {
  socks[1].close();
  const med = (g) => (g.length ? g.slice().sort((a, b) => a - b)[Math.floor(g.length / 2)] : 'n/a');
  for (let i = 0; i < 2; i++) {
    fs.appendFileSync(out, '# RECAP R' + (i + 1) + ' datagrammes=' + stats[i].n +
      ' octets=' + stats[i].bytes + ' premier=' + stats[i].first + 'ms dernier=' +
      stats[i].last + 'ms ecart_median=' + med(stats[i].gaps) + 'ms' +
      (i === 0 ? ' (retire a ' + retireAt + 'ms)' : '') + '\n');
    console.error('R' + (i + 1) + ': ' + stats[i].n + ' datagrammes, ' + stats[i].bytes + ' octets');
  }
  process.exit(0);
}, dur);
