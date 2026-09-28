(async()=>{
 const sleep=m=>new Promise(r=>setTimeout(r,m));
 const out=[];
 const ipc=require('electron').ipcRenderer;
 // 1) start logging
 ipc.emit('mnu-action',{type:'mnu-action',action:'setLogFile',data:'/tmp/nmeasim-test/sim.nmeasim'});
 out.push('setLogFile emitted');
 await sleep(3500);
 // 2) find engine Start buttons and click them
 const starts=[...document.querySelectorAll('button')].filter(b=>b.innerText.trim()==='Start');
 out.push('Start buttons found: '+starts.length);
 starts.forEach(b=>b.click());
 await sleep(2500);
 // 3) steering mode + arrow keys
 const cont=document.querySelector('.page-container');
 const key=(k)=>cont.dispatchEvent(new KeyboardEvent('keydown',{key:k,bubbles:true}));
 const tog=[...document.querySelectorAll('mat-slide-toggle')][0];
 if(tog){tog.click(); out.push('steering toggle clicked');}
 await sleep(1500);
 for(let i=0;i<10;i++) key('ArrowRight');
 await sleep(3000);
 for(let i=0;i<20;i++) key('ArrowLeft');
 await sleep(3000);
 // 4) map click to set destination
 const vp=document.querySelector('.ol-viewport');
 out.push('ol-viewport: '+(vp?Math.round(vp.getBoundingClientRect().x)+','+Math.round(vp.getBoundingClientRect().y)+' '+Math.round(vp.getBoundingClientRect().width)+'x'+Math.round(vp.getBoundingClientRect().height):'none'));
 if(vp){const r=vp.getBoundingClientRect();
   vp.dispatchEvent(new MouseEvent('click',{bubbles:true,clientX:r.x+r.width*0.8,clientY:r.y+r.height*0.25}));
   out.push('map clicked');}
 await sleep(3000);
 out.push('toolbar: '+[...document.querySelectorAll('button')].filter(x=>x.getBoundingClientRect().width>0&&x.getBoundingClientRect().y<50).map(x=>x.innerText.trim()).join(','));
 return out.join('\n');
})()
