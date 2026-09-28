#!/bin/bash
# Lance le legacy avec un TZ impose, pour instruire le comportement de ZDA.
# usage: launch-tz.sh <TZ> [autre]
set -u
cd /tmp/nmeasim-test
PAT="opt/NMEASimulator/nmeasimulator"
pkill -f "$PAT" 2>/dev/null
sleep 2
export DISPLAY=:99 HOME=/tmp/nmeasim-test
export TZ="$1"
setsid /home/tom/nmeasim-rs/analysis/deb/opt/NMEASimulator/nmeasimulator \
  --no-sandbox --user-data-dir=/tmp/nmeasim-test/profile \
  --remote-debugging-port=9222 </dev/null >>/tmp/nmeasim-test/app.log 2>&1 &
disown
sleep 9
echo "lance avec TZ=$TZ"
