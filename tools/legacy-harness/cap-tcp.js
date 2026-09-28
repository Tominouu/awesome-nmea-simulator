const net=require('net'),fs=require('fs');
const port=parseInt(process.argv[2]||'10110',10);
const out=process.argv[3]||'cap-tcp.log';
const dur=parseInt(process.argv[4]||'20000',10);
const s=net.createConnection({host:'127.0.0.1',port},()=>{
  console.error('connected to '+port);
});
const t0=Date.now();
s.on('data',b=>{ fs.appendFileSync(out, '['+(Date.now()-t0)+'ms] '+JSON.stringify(b.toString())+'\n'); });
s.on('error',e=>{console.error('ERR',e.message); process.exit(1);});
setTimeout(()=>{s.end();process.exit(0)},dur);
