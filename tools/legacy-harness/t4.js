(async()=>{
 const out=[]; const sleep=m=>new Promise(r=>setTimeout(r,m));
 try{
  require('electron').ipcRenderer.emit('mnu-action',null,{type:'mnu-action',action:'setLogFile',data:'/tmp/nmeasim-test/sim.nmeasim'});
  out.push('ipc ok');
  await sleep(3000);
  const starts=[...document.querySelectorAll('button')].filter(b=>b.innerText.trim()==='Start');
  starts.forEach(b=>b.click());
  out.push('clicked '+starts.length+' Start');
  await sleep(2500);
  out.push('checked toggles: '+[...document.querySelectorAll('mat-button-toggle')].map(t=>t.className.includes('mat-button-toggle-checked')).join(','));
  const vp=document.querySelector('.ol-viewport'); const r=vp.getBoundingClientRect();
  vp.dispatchEvent(new MouseEvent('click',{bubbles:true,clientX:r.x+r.width*0.85,clientY:r.y+r.height*0.2,view:window}));
  out.push('map click at '+Math.round(r.x+r.width*0.85)+','+Math.round(r.y+r.height*0.2));
  await sleep(3000);
  const tog=document.querySelectorAll('mat-slide-toggle')[0]; tog.click();
  out.push('steering toggled');
  await sleep(1500);
  const cont=document.querySelector('.page-container');
  for(let i=0;i<15;i++) cont.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true}));
  await sleep(4000);
 }catch(e){ out.push('EXC: '+e.message) }
 return out.join('\n');
})()
