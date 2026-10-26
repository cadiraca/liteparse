#!/usr/bin/env bash
# Smoke test for liteparse-server.
# Usage: BASE_URL=http://liteparse.carlab.local API_KEY=... scripts/smoke_test_server.sh [file.pdf]
set -u
BASE_URL="${BASE_URL:-http://localhost:8080}"
PDF="${1:-integration_tests_data/rotated_map_labels.pdf}"
CURL=(curl -s -m 60)
[ -n "${RESOLVE:-}" ] && CURL+=(--resolve "$RESOLVE")   # e.g. RESOLVE=liteparse.carlab.local:80:127.0.0.1
AUTH=()
[ -n "${API_KEY:-}" ] && AUTH=(-H "x-api-key: $API_KEY")
fail=0

check() { # name, condition-exit-code
  if [ "$2" -eq 0 ]; then echo "PASS $1"; else echo "FAIL $1"; fail=1; fi
}

body=$("${CURL[@]}" "$BASE_URL/health")
echo "$body" | grep -q '"status":"ok"'; check "health returns ok" $?

if [ -n "${API_KEY:-}" ]; then
  code=$("${CURL[@]}" -o /dev/null -w '%{http_code}' -X POST --data-binary @"$PDF" "$BASE_URL/parse")
  [ "$code" = 401 ]; check "parse without key is 401 (got $code)" $?
  code=$("${CURL[@]}" -o /dev/null -w '%{http_code}' -X POST -H "x-api-key: wrong" --data-binary @"$PDF" "$BASE_URL/parse")
  [ "$code" = 401 ]; check "parse with wrong key is 401 (got $code)" $?
fi

out=$("${CURL[@]}" "${AUTH[@]}" -F file=@"$PDF" "$BASE_URL/parse?format=text")
echo "$out" | grep -q -- '--- Page 1 ---'; check "multipart text parse" $?

out=$("${CURL[@]}" "${AUTH[@]}" --data-binary @"$PDF" "$BASE_URL/parse?ocr=false")
echo "$out" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d["total_pages"]>=1 and d["pages"]' 2>/dev/null
check "raw-body json parse" $?

out=$("${CURL[@]}" "${AUTH[@]}" --data-binary @"$PDF" "$BASE_URL/parse?format=markdown&ocr=false")
[ -n "$out" ]; check "markdown parse non-empty" $?

code=$("${CURL[@]}" "${AUTH[@]}" -o /dev/null -w '%{http_code}' --data-binary @"$PDF" "$BASE_URL/parse?format=bogus")
[ "$code" = 400 ]; check "unknown format is 400 (got $code)" $?

code=$("${CURL[@]}" "${AUTH[@]}" -o /dev/null -w '%{http_code}' -X POST "$BASE_URL/parse")
[ "$code" = 400 ]; check "empty body is 400 (got $code)" $?

code=$("${CURL[@]}" "${AUTH[@]}" -o /dev/null -w '%{http_code}' -d garbage "$BASE_URL/parse")
[ "$code" = 422 ]; check "garbage body is 422 (got $code)" $?

exit $fail
