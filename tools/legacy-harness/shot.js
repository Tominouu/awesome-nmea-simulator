const WebSocket=require('/home/tom/node_modules/ws'),http=require('http'),fs=require('fs');
const out=process.argv[2]||'/tmp/shot.png';
const clip=process.argv[3]?JSON.parse(process.argv[3]):undefined;
http.get('http://127.0.0.1:9222/json/list',r=>{let d='';r.on('data',c=>d+=c);r.on('end',()=>{
 const p=JSON.parse(d).find(x=>x.type==='page');const ws=new WebSocket(p.webSocketDebuggerUrl,{maxPayload:1<<26});
 let id=0;const pend=new Map();
 ws.on('message',m=>{const o=JSON.parse(m);if(o.id&&pend.has(o.id)){pend.get(o.id)(o);pend.delete(o.id)}});
 ws.on('open',async()=>{
  const send=(method,params)=>new Promise(res=>{const i=++id;pend.set(i,res);ws.send(JSON.stringify({id:i,method,params:params||{}}))});
  const met=await send('Page.getLayoutMetrics');
  const w=Math.ceil(met.result.cssContentSize.width),h=Math.ceil(met.result.cssContentSize.height);
  const shot=await send('Page.captureScreenshot',{format:'png',captureBeyondViewport:true,clip:clip||{x:0,y:0,width:Math.min(w,1600),height:h,scale:1}});
  fs.writeFileSync(out,Buffer.from(shot.result.data,'base64'));
  console.log('capture '+out+' taille='+w+'x'+h);process.exit(0);
 });
});});
