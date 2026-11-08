#!/usr/bin/env bash
# M1 end-to-end: one Knots-BLAKE2b regtest node, one Shoal node (P2Poolv2 fork), one CPU miner.
# Proves: XBT jobs are minable, shares form a sharechain, and blocks the miner finds are
# accepted by Knots with a coinbase that pays the sharechain's PPLNS split.
#
# Regtest only. Own datadir and ports; never touches the lab's shared rig. Everything it starts
# runs under a time cap and is stopped on exit.
#
# Env: KNOTS_BIN (dir with bitcoind/bitcoin-cli), SHOAL_NODE (p2poolv2 binary),
#      SHOAL_CLI (p2poolv2_cli binary), GRIND (shoal-grind binary), SECS (miner run, default 300)
set -euo pipefail
KNOTS_BIN=${KNOTS_BIN:-$HOME/xbt-lab/prefix/bin}
SHOAL_NODE=${SHOAL_NODE:-$HOME/shoal/p2poolv2/target/debug/p2poolv2}
SHOAL_CLI=${SHOAL_CLI:-$HOME/shoal/p2poolv2/target/debug/p2poolv2_cli}
GRIND=${GRIND:-$HOME/shoal/shoal/target/release/shoal-grind}
SECS=${SECS:-300}
THREADS=${THREADS:-3}
RUN=${RUN:-$HOME/shoal/e2e/run-$(date -u +%Y%m%dT%H%M%SZ)}
RPC_PORT=38511 P2P_PORT=38501 ZMQ_PORT=28501 STRATUM_PORT=33501 SHARE_P2P=6901 API_PORT=46901

mkdir -p "$RUN"/{node,shoal,logs}
PIDS=()
cleanup() {
  set +e
  for p in "${PIDS[@]}"; do kill -TERM -- "-$p" 2>/dev/null || kill "$p" 2>/dev/null; done
  sleep 2
  for p in "${PIDS[@]}"; do kill -KILL -- "-$p" 2>/dev/null; done
  "$KNOTS_BIN/bitcoin-cli" -datadir="$RUN/node" stop >/dev/null 2>&1
  sleep 2
  echo "cleanup done; run dir $RUN"
}
trap cleanup EXIT
trap 'exit 130' INT; trap 'exit 143' TERM; trap 'exit 129' HUP

cat > "$RUN/node/bitcoin.conf" <<CONF
regtest=1
[regtest]
blake2b_headline=Lazarus
port=$P2P_PORT
rpcport=$RPC_PORT
rpcbind=127.0.0.1
rpcallowip=127.0.0.1
rpcuser=p2pool
rpcpassword=p2pool
listen=0
connect=0
dnsseed=0
zmqpubhashblock=tcp://127.0.0.1:$ZMQ_PORT
testactivationheight=blake2b@101
rdtsexpiry=4102444800
testcoinbasematuritylong=200:200:6680
fallbackfee=0.0001
CONF
cli() { "$KNOTS_BIN/bitcoin-cli" -datadir="$RUN/node" "$@"; }
"$KNOTS_BIN/bitcoind" -datadir="$RUN/node" -daemon >/dev/null
for _ in $(seq 60); do cli getblockcount >/dev/null 2>&1 && break; sleep 0.5; done
cli createwallet shoal >/dev/null
PAYOUT=$(cli -rpcwallet=shoal getnewaddress "" bech32)
TAPROOT=$(cli -rpcwallet=shoal getnewaddress "" bech32m)
WP=$(cli -rpcwallet=shoal getaddressinfo "$TAPROOT" | python3 -c 'import json,sys;print(json.load(sys.stdin)["witness_program"])')
SHARE_ADDR=$("$SHOAL_CLI" address "$WP" --network regtest | tail -1)
cli generatetodescriptor 120 'raw(51)' >/dev/null
echo "node height $(cli getblockcount), payout $PAYOUT, share address $SHARE_ADDR"

cat > "$RUN/shoal/config.toml" <<CONF
[network]
listen_address = "/ip4/127.0.0.1/tcp/$SHARE_P2P"
max_pending_incoming = 10
max_pending_outgoing = 10
max_established_incoming = 50
max_established_outgoing = 50
max_established_per_peer = 2
max_workbase_per_second = 10
max_userworkbase_per_second = 10
max_miningshare_per_second = 100
max_inventory_per_second = 100
max_transaction_per_second = 100
max_requests_per_second = 100
dial_timeout_secs = 30

[store]
path = "$RUN/shoal/store.db"
pplns_ttl_days = 7

[stratum]
hostname = "127.0.0.1"
port = $STRATUM_PORT
start_difficulty = 1
minimum_difficulty = 1
solo_address = "$PAYOUT"
bootstrap_address = "$PAYOUT"
zmqpubhashblock = "tcp://127.0.0.1:$ZMQ_PORT"
network = "regtest"
version_mask = "00000000"
pool_signature = "Shoal"
# The first node of a new sharechain has nobody to sync from.
wait_for_chain_sync = false

[bitcoinrpc]
url = "http://127.0.0.1:$RPC_PORT"
username = "p2pool"
password = "p2pool"

[logging]
file = "$RUN/logs/shoal.log"
console = false
level = "${LOG_LEVEL:-info}"
stats_dir = "$RUN/logs/stats"

[api]
hostname = "127.0.0.1"
port = $API_PORT
CONF

setsid timeout -k 10 $((SECS + 120)) "$SHOAL_NODE" --config "$RUN/shoal/config.toml" >"$RUN/logs/shoal.out" 2>&1 &
PIDS+=($!)
UP=0
for _ in $(seq 90); do
  if (exec 3<>/dev/tcp/127.0.0.1/$STRATUM_PORT) 2>/dev/null; then UP=1; break; fi
  sleep 1
done
if [ "$UP" != 1 ]; then
  echo "FAIL: shoal stratum never listened on $STRATUM_PORT; last log lines:"
  tail -20 "$RUN"/logs/shoal.log* 2>/dev/null | cut -c1-250
  echo "RESULT found=0 paid=0 stratum=down"
  exit 1
fi
echo "shoal stratum up on $STRATUM_PORT"
START_HEIGHT=$(cli getblockcount)

setsid timeout -k 5 "$SECS" "$GRIND" --port "$STRATUM_PORT" --user "$PAYOUT.cpu1" \
  --pass "p2p=$SHARE_ADDR" --threads "$THREADS" --secs "$SECS" >"$RUN/logs/grind.log" 2>&1 &
GPID=$!; PIDS+=($GPID)
wait "$GPID" || true

END_HEIGHT=$(cli getblockcount)
echo "blocks: $START_HEIGHT -> $END_HEIGHT"
echo "miner: $(grep -c '^accepted' "$RUN/logs/grind.log") accepted, $(grep -c '^rejected' "$RUN/logs/grind.log") rejected"
grep -E "^rejected" "$RUN/logs/grind.log" | sort | uniq -c | sort -rn | head -5 || true
# Stop the Shoal node (the CLI reads its store offline), keep Knots up for the check.
kill -TERM -- "-${PIDS[0]}" 2>/dev/null || true
for _ in $(seq 30); do kill -0 "${PIDS[0]}" 2>/dev/null || break; sleep 1; done
python3 "$(dirname "$0")/verify_pplns.py" --cli "$SHOAL_CLI" --db "$RUN/shoal/store.db" \
  --bitcoin-cli "$KNOTS_BIN/bitcoin-cli" --datadir "$RUN/node" --network regtest | tee "$RUN/pplns.txt"
echo "RESULT blocks=$((END_HEIGHT - START_HEIGHT)) $(tail -1 "$RUN/pplns.txt")"
