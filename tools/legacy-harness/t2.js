(()=>{ try{ const e=require('electron'); return 'ok ipc='+(!!e.ipcRenderer)+' req='+(!!require); }catch(x){ return 'ERR '+x.message } })()
