#!/bin/bash
# Import KML par le vrai <input type=file> "streetview" (index 1).
# Pour chaque fichier : relance, injection, contenu du dialogue, selection de
# la premiere trace si elle existe, etat de la barre ; pour gx:Track on
# demarre et on capture la position emise.
set -u
cd /tmp/nmeasim-test
for F in gxtrack linestring folder; do
  echo "=================== $F.kml"
  bash launch.sh >/dev/null
  sleep 2
  rm -f console-kml-$F.log
  node consolecap.js 45 console-kml-$F.log >/dev/null 2>&1 &
  CC=$!
  sleep 2
  node setfile.js 1 /tmp/nmeasim-test/kml/$F.kml
  sleep 3
  echo "### dialogue"
  node cdp.js "(()=>{const d=document.querySelector('mat-dialog-container');return d?d.innerText.replace(/\s+/g,' ').slice(0,500):'PAS DE DIALOGUE'})()"
  echo "### traces proposees"
  node cdp.js "[...document.querySelectorAll('mat-dialog-container mat-list-item, mat-dialog-container .mat-mdc-list-item')].map(e=>e.innerText.replace(/\s+/g,' ').trim()).join(' | ')||'AUCUNE'"
  RECT=$(node cdp.js "(()=>{const i=document.querySelector('mat-dialog-container mat-list-item, mat-dialog-container .mat-mdc-list-item');if(!i)return 'ABSENT';const r=i.getBoundingClientRect();return Math.round(r.x+r.width/2)+' '+Math.round(r.y+r.height/2)})()" | tr -d '"\n')
  if [ "$RECT" != "ABSENT" ]; then
    set -- $RECT
    printf '[{"method":"Input.dispatchMouseEvent","params":{"type":"mouseMoved","x":%s,"y":%s},"wait":300},{"method":"Input.dispatchMouseEvent","params":{"type":"mousePressed","x":%s,"y":%s,"button":"left","clickCount":1},"wait":200},{"method":"Input.dispatchMouseEvent","params":{"type":"mouseReleased","x":%s,"y":%s,"button":"left","clickCount":1},"wait":800}]' $1 $2 $1 $2 $1 $2 > act-click.json
    node input.js act-click.json >/dev/null 2>&1
    echo "    premiere trace selectionnee"
  else
    bash clickicon.sh mat-dialog-container close >/dev/null 2>&1
    node cdp.js "(()=>{const b=[...document.querySelectorAll('mat-dialog-container button')];return b.map(x=>x.innerText.trim()).join('|')})()"
  fi
  sleep 2
  node cdp.js "document.querySelector('mat-dialog-container')?'dialogue encore ouvert':'dialogue ferme'"
  echo "### barre d'outils"
  node cdp.js "[...document.querySelectorAll('[mattooltip]')].map(e=>e.getAttribute('mattooltip')).join(' ~ ')"
  if [ "$F" = "gxtrack" ] && [ "$RECT" != "ABSENT" ]; then
    rm -f cap-kml-$F.log
    node cap-tcp-wait.js 10110 cap-kml-$F.log 9 20 &
    CAP=$!
    sleep 2
    bash start.sh
    wait $CAP
    echo "### positions emises (GLL)"
    grep -o 'GPGLL,[^,]*,[NS],[^,]*,[EW]' cap-kml-$F.log | head -12
    echo "### fin de trace ?"
    node cdp.js "(()=>{const t=document.body.innerText;return (t.match(/End of track[^\n]*/)||['pas de message'])[0]})()"
    node cdp.js "[...document.querySelectorAll('[mattooltip]')].map(e=>e.getAttribute('mattooltip')).filter(x=>/Simulator/.test(x)).join(' ~ ')"
  fi
  kill $CC 2>/dev/null
  echo "### console"
  grep -v 'consolecap armed' console-kml-$F.log 2>/dev/null | cut -c1-300 | head -12
done
