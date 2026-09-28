(()=>{
  // 1) applySeed parity distribution test
  let even=0,odd=0; const seq=[];
  for(let i=0;i<30;i++){const r=Math.random(); const b=Math.floor(10*r)%2==0; b?even++:odd++; seq.push(b?0:1)}
  // 2) simulate rpm random walk with seed 600 plus/minus .02
  let v=600; const walk=[];
  for(let i=0;i<12;i++){const r=Math.random(); const up=Math.floor(10*r)%2==0;
    v = (0===v)? (up? v+0.02 : v-0.02) : (up? v*(1+0.02) : v/(1+0.02));
    v=Math.max(Math.min(6000,v),600); walk.push(+v.toFixed(1));}
  return 'even='+even+' odd='+odd+'\nparity seq: '+seq.join('')+'\nwalk: '+walk.join(', ');
})()
