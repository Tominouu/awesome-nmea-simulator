#!/bin/bash
# Demarre le simulateur via le bouton de barre d'outils "Start Simulator",
# avec verification de l'effet et reprise.
set -u
cd /tmp/nmeasim-test
for i in 1 2 3 4; do
  STATE=$(node cdp.js "(()=>{const t=[...document.querySelectorAll('[mattooltip]')].map(e=>e.getAttribute('mattooltip'));return t.some(x=>/Stop Simulator/.test(x))?'EN_MARCHE':(t.some(x=>/Start Simulator/.test(x))?'ARRET':'INCONNU')})()" 2>/dev/null | tr -d '\n')
  if [ "$STATE" = "EN_MARCHE" ]; then echo "    demarrage confirme (essai $i)"; exit 0; fi
  RECT=$(node cdp.js "(()=>{const b=[...document.querySelectorAll('[mattooltip]')].find(e=>/Start Simulator/.test(e.getAttribute('mattooltip')));if(!b)return 'ABSENT';const r=b.getBoundingClientRect();return Math.round(r.x+r.width/2)+' '+Math.round(r.y+r.height/2)})()" 2>/dev/null)
  set -- $RECT
  if [ "$RECT" = "ABSENT" ] || [ -z "$1" ] || [ -z "$2" ]; then echo "    bouton introuvable (essai $i)"; sleep 2; continue; fi
  cat > act-start.json <<JSON
[
 {"method":"Input.dispatchMouseEvent","params":{"type":"mouseMoved","x":$1,"y":$2},"wait":400},
 {"method":"Input.dispatchMouseEvent","params":{"type":"mousePressed","x":$1,"y":$2,"button":"left","clickCount":1},"wait":400},
 {"method":"Input.dispatchMouseEvent","params":{"type":"mouseReleased","x":$1,"y":$2,"button":"left","clickCount":1},"wait":1500}
]
JSON
  node input.js act-start.json >/dev/null 2>&1
done
STATE=$(node cdp.js "(()=>{const t=[...document.querySelectorAll('[mattooltip]')].map(e=>e.getAttribute('mattooltip'));return t.some(x=>/Stop Simulator/.test(x))?'EN_MARCHE':'ECHEC'})()" 2>/dev/null | tr -d '\n')
echo "    etat final: $STATE"
[ "$STATE" = "EN_MARCHE" ]
