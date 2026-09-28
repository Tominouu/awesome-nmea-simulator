const dgram=require('dgram'),fs=require('fs');
const port=parseInt(process.argv[2]||'3100',10);
const out=process.argv[3]||'cap-udp2.log';
const dur=parseInt(process.argv[4]||'10000',10);
const s=dgram.createSocket({type:'udp4',reuseAddr:true});
const t0=Date.now();
s.on('message',(b,r)=>{fs.appendFileSync(out,'['+(Date.now()-t0)+'ms] from '+r.address+':'+r.port+' len='+b.length+' '+JSON.stringify(b.toString().slice(0,300))+'\n')});
s.bind(port,()=>{s.setBroadcast(true);console.error('udp listening '+port)});
setTimeout(()=>{s.close();process.exit(0)},dur);
