const WebSocket=require('/home/tom/node_modules/ws');
const http=require('http');
const expr=require('fs').readFileSync(process.argv[2],'utf8');
const wait=Number(process.argv[3]||15000);
http.get('http://127.0.0.1:9222/json/list',res=>{let d='';res.on('data',c=>d+=c);res.on('end',()=>{
  const page=JSON.parse(d).find(x=>x.type==='page');
  const ws=new WebSocket(page.webSocketDebuggerUrl);
  ws.on('open',()=>{ws.send(JSON.stringify({id:1,method:'Runtime.evaluate',params:{expression:expr,returnByValue:true,awaitPromise:true,userGesture:true}}))});
  ws.on('message',m=>{const o=JSON.parse(m);
    if(o.id===1){console.log(o.result&&o.result.result?JSON.stringify(o.result.result.value,null,1):JSON.stringify(o.result));
      setTimeout(()=>{ws.close();process.exit(0)},500);}});
  setTimeout(()=>{ws.close();process.exit(0)},wait);
});});
