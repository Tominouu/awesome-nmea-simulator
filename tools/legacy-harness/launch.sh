#!/bin/bash
pkill -f "opt/NMEASimulator/nmeasimulator" 2>/dev/null
sleep 2
export DISPLAY=:99 HOME=/tmp/nmeasim-test
setsid /home/tom/nmeasim-rs/analysis/deb/opt/NMEASimulator/nmeasimulator \
  --no-sandbox --user-data-dir=/tmp/nmeasim-test/profile \
  --remote-debugging-port=9222 </dev/null >>/tmp/nmeasim-test/app.log 2>&1 &
disown
sleep 9
echo "launched"
