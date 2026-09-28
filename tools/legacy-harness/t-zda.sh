#!/bin/bash
# Comportement de ZDA sous plusieurs fuseaux. autoStart=false, demarrage manuel.
set -u
cd /tmp/nmeasim-test
OUT=zda-tz.log
: > $OUT
for TZNAME in UTC Asia/Tokyo America/St_Johns Asia/Kolkata Pacific/Chatham Europe/Paris; do
  echo "=== TZ=$TZNAME"
  TZZ="$TZNAME" bash caprun.sh cfg-zda.json "node cap-tcp-wait.js 10110 zda-one.log 18 40" zda-one.log 16 2>&1 | grep -E "demarrage|etat final|confirme|ECHEC" | sed 's/^/    /'
  Z=$(grep -o '\$GPZDA,[^\\]*' zda-one.log 2>/dev/null | head -1)
  R=$(grep -o '\$GPRMC,[0-9.]*,A' zda-one.log 2>/dev/null | head -1)
  D=$(grep -o '\$GPZDA,[^,]*,[^,]*,[^,]*,[^,]*,[^,]*,[^,]*' zda-one.log 2>/dev/null | head -1)
  echo "    RMC : ${R:-VIDE}"
  echo "    ZDA : ${Z:-VIDE}"
  printf '%s\t%s\t%s\n' "$TZNAME" "$R" "$Z" >> $OUT
  rm -f zda-one.log
done
echo
echo "=== recapitulatif ==="
printf '%-20s %-24s %s\n' "FUSEAU" "RMC (UTC)" "ZDA complet"
while IFS=$'\t' read -r t r z; do printf '%-20s %-24s %s\n' "$t" "${r#\$GPRMC,}" "${z#\$GPZDA,}"; done < $OUT
