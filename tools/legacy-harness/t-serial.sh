#!/bin/bash
# Transport serie (type 3) sur une paire de PTY creee par os.openpty().
# socat est absent : pty-serial.py tient le maitre, l'application ouvre l'esclave.
set -u
cd /tmp/nmeasim-test
rm -f pty.path cap-serial.log
python3 pty-serial.py pty.path cap-serial.log 75 10 > pty-serial.out 2>&1 &
PTY=$!
for i in $(seq 1 20); do [ -s pty.path ] && break; sleep 0.2; done
P=$(cat pty.path); echo "PTY esclave : $P"
node -e '
const fs=require("fs");const c=JSON.parse(fs.readFileSync("cfg-zda.json"));
c.server.type=3; c.server.serial={port:process.argv[1],baudRate:4800,dataBits:8,stopBits:1,parity:"none"};
c.server.manualSerialPort=true; c.server.manualBaudRate=false; c.autoStart=false;
fs.writeFileSync("cfg-serial.json",JSON.stringify(c));' "$P"
CAPCONSOLE=1 bash caprun.sh cfg-serial.json "sleep 20" serial-dummy.log 20 2>&1 | grep -v '^### recap'
echo "### config relue par l'application"
node cdp.js "(()=>{const c=JSON.parse(localStorage.getItem('nmeasim_config'));return JSON.stringify({type:c.server.type,serial:c.server.serial,manual:c.server.manualSerialPort})})()"
echo "### arret"
RECT=$(node cdp.js "(()=>{const b=[...document.querySelectorAll('[mattooltip]')].find(e=>/Stop Simulator/.test(e.getAttribute('mattooltip')));if(!b)return 'ABSENT';const r=b.getBoundingClientRect();return Math.round(r.x+r.width/2)+' '+Math.round(r.y+r.height/2)})()" | tr -d '"\n')
if [ "$RECT" != "ABSENT" ]; then set -- $RECT
 printf '[{"method":"Input.dispatchMouseEvent","params":{"type":"mouseMoved","x":%s,"y":%s},"wait":300},{"method":"Input.dispatchMouseEvent","params":{"type":"mousePressed","x":%s,"y":%s,"button":"left","clickCount":1},"wait":200},{"method":"Input.dispatchMouseEvent","params":{"type":"mouseReleased","x":%s,"y":%s,"button":"left","clickCount":1},"wait":1500}]' $1 $2 $1 $2 $1 $2 > act-click.json
 node input.js act-click.json >/dev/null 2>&1; fi
node cdp.js "[...document.querySelectorAll('[mattooltip]')].map(e=>e.getAttribute('mattooltip')).filter(x=>/Simulator/.test(x)).join(' ~ ')"
echo "### page Settings (refreshSerialPorts sans port detecte)"
RECT=$(node cdp.js "(()=>{const a=document.querySelector('a[href*=settings]');if(!a)return 'ABSENT';const r=a.getBoundingClientRect();return Math.round(r.x+r.width/2)+' '+Math.round(r.y+r.height/2)})()" | tr -d '"\n')
echo "    lien settings -> $RECT"
if [ "$RECT" != "ABSENT" ]; then set -- $RECT
 printf '[{"method":"Input.dispatchMouseEvent","params":{"type":"mouseMoved","x":%s,"y":%s},"wait":300},{"method":"Input.dispatchMouseEvent","params":{"type":"mousePressed","x":%s,"y":%s,"button":"left","clickCount":1},"wait":200},{"method":"Input.dispatchMouseEvent","params":{"type":"mouseReleased","x":%s,"y":%s,"button":"left","clickCount":1},"wait":3000}]' $1 $2 $1 $2 $1 $2 > act-click.json
 node input.js act-click.json >/dev/null 2>&1; fi
node cdp.js "[...document.querySelectorAll('mat-select')].map(s=>s.innerText.trim()).join(' | ')"
kill $PTY 2>/dev/null; wait $PTY 2>/dev/null
cat pty-serial.out
echo "### cap-serial.log : $(wc -l < cap-serial.log) lectures"
