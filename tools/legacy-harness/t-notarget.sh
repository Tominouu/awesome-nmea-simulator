#!/bin/bash
# Verifie le comportement du mode tcpclient quand la cible est absente.
set -u
cd /tmp/nmeasim-test
PAT="opt/NMEASimulator/nmeasimulator"

echo "### port 15001 : $(ss -ltn 2>/dev/null | grep -c 15001) ecoute(s)"

bash launch.sh >/dev/null
node cdp.js "$(node seed.js cfg-tcpclient.json)" >/dev/null
sleep 9
bash launch.sh >/dev/null
sleep 3

rm -f console-notarget.log
node consolecap.js 24 console-notarget.log >/dev/null 2>&1 &
CC=$!
sleep 2

node cdp.js "(()=>{const b=[...document.querySelectorAll('[mattooltip]')].find(e=>/Start Simulator/.test(e.getAttribute('mattooltip')));return b?'bouton Start present':'DEJA en marche'})()"
node input.js act-start.json >/dev/null 2>&1
sleep 18

echo "### barre d'outils apres 18 s sans serveur"
node cdp.js "(()=>{const t=[...document.querySelectorAll('[mattooltip]')].map(e=>e.getAttribute('mattooltip')).filter(x=>/Simulator|Logging/.test(x));return '  barre: '+t.join(' ~ ')})()"

echo "### journal envoye ?"
node cdp.js "(()=>{const b=[...document.querySelectorAll('mat-button-toggle')].map(t=>t.getAttribute('aria-checked')+':'+t.textContent.trim());return '  toggles: '+b.join(' | ')})()"

wait $CC 2>/dev/null
echo "### console renderer (filtre reseau)"
grep -Ei "ECONN|ECONNRESET|error|warn|Failed|client|stopped" console-notarget.log | head -10 | sed 's/^/  /'

echo "### processus encore vivant ?"
pgrep -f "$PAT" >/dev/null && echo "  OUI, vivant" || echo "  NON, mort"
echo "### app.log (filtre reseau, 20 dernieres)"
tail -40 app.log | grep -Ei "network|error: " | tail -5 | sed 's/^/  /'
