#!/bin/bash
# Pilote de capture : lance le legacy, injecte la config, demarre le recepteur,
# clique Start, laisse tourner, puis restitue l'etat du moteur.
# usage: caprun.sh <cfg.json> "<commande recepteur>" <sortie> [duree_cap_s]
set -u
cd /tmp/nmeasim-test
CFG=$1
RCV=$2
OUT=$3
DUR=${4:-20}
LOG=app.log

# TZZ defini : lance le legacy avec un fuseau impose
if [ -n "${TZZ:-}" ]; then
  LAUNCH="bash launch-tz.sh $TZZ >/dev/null"
else
  LAUNCH="bash launch.sh >/dev/null"
fi

echo "### marqueur $(date -Is)  phase1=relaunch+seed  cfg=$(basename $CFG)"
MARK1=$(wc -l < $LOG)
$LAUNCH
node cdp.js "$(node seed.js $CFG)" >/dev/null || { echo "ECHEC injection"; exit 1; }
sleep 9
echo "### marqueur $(date -Is)  phase2=relaunch  (l'app relit nmeasim_config)"
MARK2=$(wc -l < $LOG)
$LAUNCH
sleep 2

rm -f "$OUT"
echo "### marqueur $(date -Is)  phase3=recepteur  -> $OUT"
if [ "${CAPCONSOLE:-0}" = "1" ]; then
  rm -f "console-$(basename $OUT .log).log"
  node consolecap.js "$((DUR+8))" "console-$(basename $OUT .log).log" >/dev/null 2>&1 &
  sleep 2
fi
eval "$RCV" &
RCV_PID=$!
sleep 3

echo "### marqueur $(date -Is)  phase4=clic Start (bouton toolbar 'Start Simulator')"
node cdp.js "(()=>{const b=[...document.querySelectorAll('[mattooltip]')].find(e=>/Start Simulator/.test(e.getAttribute('mattooltip')));return b?JSON.stringify(b.getBoundingClientRect()):'ABSENT'})()" | sed 's/^/    /'
cat > /tmp/nmeasim-test/act-start.json <<'JSON'
[
 {"method":"Input.dispatchMouseEvent","params":{"type":"mouseMoved","x":1176,"y":24},"wait":400},
 {"method":"Input.dispatchMouseEvent","params":{"type":"mousePressed","x":1176,"y":24,"button":"left","clickCount":1},"wait":300},
 {"method":"Input.dispatchMouseEvent","params":{"type":"mouseReleased","x":1176,"y":24,"button":"left","clickCount":1},"wait":2000}
]
JSON
bash start.sh
node cdp.js "(()=>{const t=[...document.querySelectorAll('[mattooltip]')].map(e=>e.getAttribute('mattooltip')).filter(x=>/Simulator|Logging/.test(x));return 'barre: '+t.join(' ~ ')})()" | sed 's/^/    /'

wait $RCV_PID 2>/dev/null
echo "### marqueur $(date -Is)  phase5=fin capture"
node cdp.js "(()=>{const t=[...document.querySelectorAll('[mattooltip]')].map(e=>e.getAttribute('mattooltip')).filter(x=>/Simulator|Logging/.test(x));return 'barre: '+t.join(' ~ ')})()" | sed 's/^/    /'

echo "### app.log phase1 (injection)"; sed -n "$((MARK1+1)),${MARK2}p" $LOG | grep -Ei "server|client|multicast|error|warn" | sed 's/^/    /'
echo "### app.log phase2 (capture)";  sed -n "$((MARK2+1)),\$p"   $LOG | grep -Ei "server|client|multicast|error|warn" | sed 's/^/    /'
echo "### recap $OUT : $(wc -l < "$OUT") lignes, $(wc -c < "$OUT") octets"
