const WebSocket=require('/home/tom/node_modules/ws');
const http=require('http');
const expr=process.argv[2]||'1';
http.get('http://127.0.0.1:9222/json/list',res=>{let d='';res.on('data',c=>d+=c);res.on('end',()=>{
  const page=JSON.parse(d).find(x=>x.type==='page');
  const ws=new WebSocket(page.webSocketDebuggerUrl);
  let id=0; const pend={}; const logs=[];
  ws.on('open',()=>{
    ws.send(JSON.stringify({id:++id,method:'Runtime.enable'}));
    ws.send(JSON.stringify({id:++id,method:'Log.enable'}));
    ws.send(JSON.stringify({id:++id,method:'Runtime.evaluate',params:{expression:expr,returnByValue:true,awaitPromise:true}}));
  });
  ws.on('message',m=>{const o=JSON.parse(m);
    if(o.method==='Runtime.consoleAPICalled'){logs.push('['+o.params.type+'] '+o.params.args.map(a=>a.value!==undefined?a.value:(a.description||a.type)).join(' '));}
    if(o.method==='Log.entryAdded'){logs.push('[log:'+o.params.entry.level+'] '+o.params.entry.text);}
    if(o.id&&o.result&&o.result.result){console.log('RESULT: '+JSON.stringify(o.result.result.value));}
    if(o.id&&o.result&&o.result.exceptionDetails){console.log('EXC: '+JSON.stringify(o.result.exceptionDetails));}
  });
  setTimeout(()=>{console.log('--- CONSOLE ('+logs.length+') ---');console.log(logs.slice(0,120).join('\n'));ws.close();process.exit(0);},Number(process.argv[3]||4000));
});});
