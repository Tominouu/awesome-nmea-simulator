#!/bin/bash
# Cycle propre et instrruit : relance, config, capture console, demarrage,
# client TCP, observation.
set -u
cd /tmp/nmeasim-test
PAT="opt/NMEASimulator/nmeasimulator"

echo "### 1. arret + relance propre"
pkill -f "$PAT" 2>/dev/null; sleep 2
export DISPLAY=:99 HOME=/tmp/nmeasim-test
unset TZ
setsid /home/tom/nmeasim-rs/analysis/deb/opt/NMEASimulator/nmeasimulator \
  --no-sandbox --user-data-dir=/tmp/nmeasim-test/profile \
  --remote-debugging-port=9222 </dev/null >>/tmp/nmeasim-test/app.log 2>&1 &
disown
sleep 9

echo "### 2. config"
node cdp.js "$(node seed.js cfg-tcp.json)" >/dev/null
sleep 9
pkill -f "$PAT" 2>/dev/null; sleep 2
setsid /home/tom/nmeasim-rs/analysis/deb/opt/NMEASimulator/nmeasimulator \
  --no-sandbox --user-data-dir=/tmp/nmeasim-test/profile \
  --remote-debugging-port=9222 </dev/null >>/tmp/nmeasim-test/app.log 2>&1 &
disown
sleep 10

echo "### 3. config effective"
node cdp.js "(()=>{const c=JSON.parse(localStorage.getItem('nmeasim_config'));return '    type='+c.server.type+' port='+c.server.ip.port+' autoStart='+c.autoStart})()"
echo "### 4. barre d'outils avant"
node cdp.js "    '+[...document.querySelectorAll('[mattooltip]')].map(e=>e.getAttribute('mattooltip')).filter(x=>/Simulator|Logging/.test(x)).join(' ~ ')"

echo "### 5. capture console + client TCP"
rm -f console-probe.log /tmp/p4.log
node consolecap.js 30 console-probe.log >/dev/null 2>&1 &
CC=$!
sleep 2
node cap-tcp.js 10110 /tmp/p4.log 26 2>&1 | head -1 | sed 's/^/    client: /' &
CL=$!
sleep 3

echo "### 6. clic Stop puis Start (cycle complet)"
node input.js act-stop.json >/dev/null 2>&1
sleep 3
node cdp.js "    '+[...document.querySelectorAll('[mattooltip]')].map(e=>e.getAttribute('mattooltip')).filter(x=>/Simulator|Logging/.test(x)).join(' ~ ')"
bash start.sh
sleep 2
node cdp.js "    '+[...document.querySelectorAll('[mattooltip]')].map(e=>e.getAttribute('mattooltip')).filter(x=>/Simulator|Logging/.test(x)).join(' ~ ')"

wait $CL 2>/dev/null
echo "### 7. octets recus par le client : $(wc -c < /tmp/p4.log 2>/dev/null || echo 0)"
wait $CC 2>/dev/null
echo "### 8. console renderer (transport)"
grep -Ei "server|client|port|error|warn" console-probe.log | grep -v "consolecap armed" | head -12 | sed 's/^/    /'
echo "### 9. listening"
ss -ltn 2>/dev/null | grep 10110 | sed 's/^/    /' || echo "    rien"
