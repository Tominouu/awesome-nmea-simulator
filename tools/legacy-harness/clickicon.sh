#!/bin/bash
# Clique (vrai Input.dispatchMouseEvent) sur le bouton dont le mat-icon vaut $2,
# dans le conteneur CSS $1. usage: clickicon.sh <selecteur> <icone>
set -u
cd /tmp/nmeasim-test
RECT=$(node cdp.js "(()=>{const i=[...document.querySelectorAll('$1 mat-icon')].find(e=>e.textContent.trim()==='$2');if(!i)return 'ABSENT';const b=i.closest('button')||i;const r=b.getBoundingClientRect();return Math.round(r.x+r.width/2)+' '+Math.round(r.y+r.height/2)+' '+(b.disabled?'disabled':'enabled')})()" | tr -d '"\n')
echo "    $2 -> $RECT"
[ "$RECT" = "ABSENT" ] && exit 1
set -- $RECT
cat > act-click.json <<JSON
[
 {"method":"Input.dispatchMouseEvent","params":{"type":"mouseMoved","x":$1,"y":$2},"wait":300},
 {"method":"Input.dispatchMouseEvent","params":{"type":"mousePressed","x":$1,"y":$2,"button":"left","clickCount":1},"wait":200},
 {"method":"Input.dispatchMouseEvent","params":{"type":"mouseReleased","x":$1,"y":$2,"button":"left","clickCount":1},"wait":500}
]
JSON
node input.js act-click.json >/dev/null 2>&1
