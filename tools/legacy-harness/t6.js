(()=>{
 const out=[];
 const togs=document.querySelectorAll('mat-slide-toggle');
 out.push('toggles='+togs.length);
 togs.forEach((t,i)=>out.push('  tgl'+i+' checked='+t.className.includes('mat-slide-toggle-checked')+' checkedProp='+(t.querySelector('input')||{}).checked));
 const cont=document.querySelector('.page-container');
 // real keyboard event via KeyboardEvent on cont
 const ev=new KeyboardEvent('keydown',{key:'ArrowRight',code:'ArrowRight',keyCode:39,which:39,bubbles:true,cancelable:true});
 cont.dispatchEvent(ev);
 out.push('keydown dispatched, defaultPrevented='+ev.defaultPrevented);
 const body=document.body.innerText;
 const m=body.match(/Rudder[\s\S]{0,40}/); out.push('helm text: '+(m?m[0].replace(/\n/g,'|'):'n/a'));
 return out.join('\n');
})()
