const fs=require('fs');
const cfg=fs.readFileSync(process.argv[2],'utf8');
const expr = `(()=>{localStorage.setItem('nmeasim_config', ${JSON.stringify(cfg)}); return 'seeded';})()`;
process.stdout.write(expr);
