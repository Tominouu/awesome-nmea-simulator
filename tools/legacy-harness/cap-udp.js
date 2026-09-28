const dgram=require('dgram'),fs=require('fs');
const port=parseInt(process.argv[2]||'7001',10);
const out=process.argv[3]||'cap-udp.log';
const dur=parseInt(process.argv[4]||'20000',10);
const s=dgram.createSocket('udp4');
const t0=Date.now();
s.on('message',(b,r)=>{fs.appendFileSync(out,'['+(Date.now()-t0)+'ms] from '+r.address+':'+r.port+' len='+b.length+' '+JSON.stringify(b.toString())+'\n');});
s.bind(port,()=>console.error('listening udp '+port));
setTimeout(()=>{s.close();process.exit(0)},dur);
