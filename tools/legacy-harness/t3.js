(async()=>{
 const out=[];
 try{
  out.push('page-container: '+!!document.querySelector('.page-container'));
  out.push('slide-toggle: '+document.querySelectorAll('mat-slide-toggle').length);
  out.push('ol-viewport: '+document.querySelectorAll('.ol-viewport').length);
  out.push('Start btns: '+[...document.querySelectorAll('button')].filter(b=>b.innerText.trim()==='Start').length);
  out.push('Stop btns: '+[...document.querySelectorAll('button')].filter(b=>b.innerText.trim()==='Stop').length);
  const b=[...document.querySelectorAll('button')].filter(x=>x.innerText.trim()==='Start');
  out.push('start parents: '+b.map(x=>x.parentElement.tagName+'/'+(x.parentElement.className||'').slice(0,40)).join(' | '));
  const ipc=require('electron').ipcRenderer;
  ipc.emit('mnu-action',{type:'mnu-action',action:'setLogFile',data:'/tmp/nmeasim-test/sim.nmeasim'});
  out.push('ipc emitted');
 }catch(e){ out.push('EXC: '+e.message+' | '+e.stack.split('\n')[1]) }
 return out.join('\n');
})()
