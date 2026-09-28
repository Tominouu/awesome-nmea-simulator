#!/bin/bash
# Rejeu d'un journal .nmeasim : injection par le vrai <input type=file>,
# lecture (play), pause, pas-a-pas, capture TCP cote client.
set -u
cd /tmp/nmeasim-test
cp -p /tmp/nmeasim-test/meta-test.nmeasim /tmp/nmeasim-test/replay-src.nmeasim
bash launch.sh >/dev/null
node cdp.js "$(node seed.js cfg-zda.json)" >/dev/null
sleep 9
bash launch.sh >/dev/null
sleep 2
rm -f cap-replay-meta.log console-replay-meta.log
node consolecap.js 70 console-replay-meta.log >/dev/null 2>&1 &
sleep 2
node cap-tcp-wait.js 10110 cap-replay-meta.log 40 30 &
CAP=$!
node setfile.js 0 /tmp/nmeasim-test/replay-src.nmeasim
sleep 4
echo "### dialogue"
node cdp.js "(()=>{const d=document.querySelector('mat-dialog-container');return d?d.innerText.replace(/\s+/g,' ').slice(0,400):'PAS DE DIALOGUE'})()"
echo "### icones du dialogue"
node cdp.js "[...document.querySelectorAll('mat-dialog-container mat-icon')].map(e=>e.textContent.trim()+(e.closest('button')&&e.closest('button').disabled?'(off)':'')).join(' ')"
sleep 3
echo "### play (t0)"; date +%T.%N
bash clickicon.sh mat-dialog-container play_arrow
sleep 8
echo "### pause"; date +%T.%N
bash clickicon.sh mat-dialog-container pause
node cdp.js "(()=>{const d=document.querySelector('mat-dialog-container');return d.innerText.match(/[Gg]roup[^\n]*/)?.[0]||d.innerText.slice(0,200)})()"
sleep 3
echo "### skip_next x2"; date +%T.%N
bash clickicon.sh mat-dialog-container skip_next; sleep 2
bash clickicon.sh mat-dialog-container skip_next; sleep 2
echo "### skip_previous"; date +%T.%N
bash clickicon.sh mat-dialog-container skip_previous; sleep 2
node cdp.js "(()=>{const d=document.querySelector('mat-dialog-container');return d.innerText.match(/[Gg]roup[^\n]*/)?.[0]||''})()"
echo "### close"; date +%T.%N
bash clickicon.sh mat-dialog-container close; sleep 2
node cdp.js "document.querySelector('mat-dialog-container')?'DIALOGUE ENCORE OUVERT':'dialogue ferme'"
wait $CAP
echo "### recap cap-replay-meta.log : $(wc -l < cap-replay-meta.log) lignes, $(wc -c < cap-replay-meta.log) octets"
