const WebSocket=require('/home/tom/node_modules/ws'),http=require('http');
const acts=JSON.parse(require('fs').readFileSync(process.argv[2],'utf8'));
http.get('http://127.0.0.1:9222/json/list',r=>{let d='';r.on('data',c=>d+=c);r.on('end',()=>{
 const p=JSON.parse(d).find(x=>x.type==='page'); const ws=new WebSocket(p.webSocketDebuggerUrl);
 let step=0;
 ws.on('open',()=>send());
 function send(){
   if(step>=acts.length){console.log('ALL DONE');setTimeout(()=>process.exit(0),200);return}
   const a=acts[step++];
   if(a.eval){ws.send(JSON.stringify({id:step,method:'Runtime.evaluate',params:{expression:a.eval,returnByValue:true,awaitPromise:true}}))}
   else if(a.getRect){ws.send(JSON.stringify({id:step,method:'Runtime.evaluate',params:{expression:a.getRect,returnByValue:true}}))}
   else{const {id,method,params}=a;ws.send(JSON.stringify({id:step,method,params}))}
   setTimeout(send, a.wait||700);
 }
 ws.on('message',m=>{const o=JSON.parse(m);
   if(o.result&&o.result.result&&o.result.result.value!==undefined)console.log('R'+o.id+': '+JSON.stringify(o.result.result.value));
   if(o.error)console.log('ERR'+o.id+': '+JSON.stringify(o.error));
 });
});});
