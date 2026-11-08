#!/usr/bin/env bash
# M2 end-to-end: three Shoal nodes, each with its own Knots-BLAKE2b regtest node and CPU miner,
# on one regtest blockchain and one sharechain. No node is special after start-up: node 1 starts
# the sharechain, nodes 2 and 3 join it over the share p2p network and sync before serving work.
#
# Proves: the nodes agree (identical sharechain tips at the end) and every block any node finds
# pays the sharechain's PPLNS split (checked by verify_pplns.py, which reads only the chains).
#
# Regtest only. Own datadirs and ports; everything runs under a time cap and is stopped on exit.
#
# Env: KNOTS_BIN, SHOAL_NODE, SHOAL_CLI, GRIND (as m1), SECS (mining time, default 1800),
#      THREADS (per miner, default 2), LOG_LEVEL
set -euo pipefail
KNOTS_BIN=${KNOTS_BIN:-$HOME/xbt-lab/prefix/bin}
SHOAL_NODE=${SHOAL_NODE:-$HOME/shoal/p2poolv2/target/debug/p2poolv2}
SHOAL_CLI=${SHOAL_CLI:-$HOME/shoal/p2poolv2/target/debug/p2poolv2_cli}
GRIND=${GRIND:-$HOME/shoal/shoal/target/release/shoal-grind}
SECS=${SECS:-1800}
THREADS=${THREADS:-2}
RUN=${RUN:-$HOME/shoal/e2e/m2-$(date -u +%Y%m%dT%H%M%SZ)}
N=3
rpc_port() { echo $((38510 + $1)); }
p2p_port() { echo $((38500 + $1)); }
zmq_port() { echo $((28500 + $1)); }
stratum_port() { echo $((33500 + $1)); }
share_port() { echo $((6900 + $1)); }
api_port() { echo $((46900 + $1)); }

PIDS=()
cleanup() {
  set +e
  for p in "${PIDS[@]}"; do kill -TERM -- "-$p" 2>/dev/null || kill "$p" 2>/dev/null; done
  sleep 3
  for p in "${PIDS[@]}"; do kill -KILL -- "-$p" 2>/dev/null; done
  for i in $(seq $N); do "$KNOTS_BIN/bitcoin-cli" -datadir="$RUN/node$i" stop >/dev/null 2>&1; done
  sleep 2
  echo "cleanup done; run dir $RUN"
}
trap cleanup EXIT
trap 'exit 130' INT; trap 'exit 143' TERM; trap 'exit 129' HUP

cli() { local i=$1; shift; "$KNOTS_BIN/bitcoin-cli" -datadir="$RUN/node$i" "$@"; }

# --- Knots: node 1 listens, nodes 2 and 3 connect to it -------------------------------------
for i in $(seq $N); do
  mkdir -p "$RUN/node$i" "$RUN/shoal$i" "$RUN/logs$i"
  if [ "$i" = 1 ]; then PEERING="listen=1
bind=127.0.0.1"; else PEERING="listen=0
connect=127.0.0.1:$(p2p_port 1)"; fi
  cat > "$RUN/node$i/bitcoin.conf" <<CONF
regtest=1
[regtest]
blake2b_headline=Lazarus
port=$(p2p_port "$i")
rpcport=$(rpc_port "$i")
rpcbind=127.0.0.1
rpcallowip=127.0.0.1
rpcuser=p2pool
rpcpassword=p2pool
$PEERING
dnsseed=0
zmqpubhashblock=tcp://127.0.0.1:$(zmq_port "$i")
testactivationheight=blake2b@101
rdtsexpiry=4102444800
testcoinbasematuritylong=200:200:6680
fallbackfee=0.0001
CONF
  "$KNOTS_BIN/bitcoind" -datadir="$RUN/node$i" -daemon >/dev/null
done
for i in $(seq $N); do
  for _ in $(seq 60); do cli "$i" getblockcount >/dev/null 2>&1 && break; sleep 0.5; done
done
# Identify the start-up "Method not found" (M1 loose end): upstream logs getdifficulty once.
if cli 1 getdifficulty >/dev/null 2>&1; then echo "knots: getdifficulty present"; else
  echo "knots: getdifficulty missing (the start-up 'Method not found')"; fi

declare -A PAYOUT SHARE_ADDR
for i in $(seq $N); do
  cli "$i" createwallet "shoal$i" >/dev/null
  PAYOUT[$i]=$(cli "$i" -rpcwallet="shoal$i" getnewaddress "" bech32)
  TAPROOT=$(cli "$i" -rpcwallet="shoal$i" getnewaddress "" bech32m)
  WP=$(cli "$i" -rpcwallet="shoal$i" getaddressinfo "$TAPROOT" | python3 -c 'import json,sys;print(json.load(sys.stdin)["witness_program"])')
  SHARE_ADDR[$i]=$("$SHOAL_CLI" address "$WP" --network regtest | tail -1)
  echo "miner $i payout ${PAYOUT[$i]}"
done
cli 1 generatetodescriptor 120 'raw(51)' >/dev/null
for i in $(seq 2 $N); do
  for _ in $(seq 60); do [ "$(cli "$i" getblockcount)" = 120 ] && break; sleep 0.5; done
  [ "$(cli "$i" getblockcount)" = 120 ] || { echo "FAIL: knots node $i did not sync"; exit 1; }
done
echo "knots: $N nodes at height 120"

# --- Shoal nodes -----------------------------------------------------------------------------
write_config() {
  local i=$1 wait=$2 dial=$3
  cat > "$RUN/shoal$i/config.toml" <<CONF
[network]
listen_address = "/ip4/127.0.0.1/tcp/$(share_port "$i")"
dial_peers = [$dial]
dns_seed = false
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
path = "$RUN/shoal$i/store.db"
pplns_ttl_days = 7

[stratum]
hostname = "127.0.0.1"
port = $(stratum_port "$i")
start_difficulty = 1
minimum_difficulty = 1
solo_address = "${PAYOUT[$i]}"
bootstrap_address = "${PAYOUT[1]}"
zmqpubhashblock = "tcp://127.0.0.1:$(zmq_port "$i")"
network = "regtest"
version_mask = "00000000"
pool_signature = "Shoal"
wait_for_chain_sync = $wait

[bitcoinrpc]
url = "http://127.0.0.1:$(rpc_port "$i")"
username = "p2pool"
password = "p2pool"

[logging]
file = "$RUN/logs$i/shoal.log"
console = false
level = "${LOG_LEVEL:-info}"
stats_dir = "$RUN/logs$i/stats"

[api]
hostname = "127.0.0.1"
port = $(api_port "$i")
CONF
}

declare -A NODE_PID MINER_PID
start_node() {
  local i=$1
  setsid timeout -k 10 $((SECS + 900)) "$SHOAL_NODE" --config "$RUN/shoal$i/config.toml" \
    >"$RUN/logs$i/shoal.out" 2>&1 &
  NODE_PID[$i]=$!; PIDS+=($!)
}
wait_stratum() {
  local i=$1 limit=$2
  for _ in $(seq "$limit"); do
    if (exec 3<>"/dev/tcp/127.0.0.1/$(stratum_port "$i")") 2>/dev/null; then return 0; fi
    sleep 1
  done
  echo "FAIL: shoal node $i stratum never listened; last log lines:"
  tail -20 "$RUN/logs$i"/shoal.log* 2>/dev/null | cut -c1-250
  return 1
}
start_miner() {
  local i=$1 secs=$2
  setsid timeout -k 5 "$secs" "$GRIND" --port "$(stratum_port "$i")" --user "${PAYOUT[$i]}.cpu$i" \
    --pass "p2p=${SHARE_ADDR[$i]}" --threads "$THREADS" --secs "$secs" >"$RUN/logs$i/grind.log" 2>&1 &
  MINER_PID[$i]=$!; PIDS+=($!)
}
START_HEIGHT=$(cli 1 getblockcount)
T0=$(date +%s)
# Node 1 starts the sharechain: nobody to sync from.
write_config 1 false ""
start_node 1
wait_stratum 1 90
echo "node 1 stratum up"
start_miner 1 "$SECS"
# Wait for node 1's first share so the chain tip is current, then let nodes 2 and 3 join and
# sync before they serve work (upstream's default wait_for_chain_sync = true).
for _ in $(seq 600); do
  [ "$(grep -c '^accepted' "$RUN/logs1/grind.log" 2>/dev/null || echo 0)" -ge 1 ] && break
  sleep 1
done
echo "node 1 first share after $(( $(date +%s) - T0 )) s"
DIAL="\"/ip4/127.0.0.1/tcp/$(share_port 1)\""
for i in $(seq 2 $N); do
  write_config "$i" true "$DIAL"
  start_node "$i"
done
for i in $(seq 2 $N); do
  wait_stratum "$i" 300
  echo "node $i synced, stratum up after $(( $(date +%s) - T0 )) s"
  start_miner "$i" $(( SECS - ($(date +%s) - T0) ))
done

# Progress every 5 minutes.
while kill -0 "${MINER_PID[1]}" 2>/dev/null; do
  sleep 300 & wait $! || true
  line="t+$(( $(date +%s) - T0 ))s blocks=$(( $(cli 1 getblockcount) - START_HEIGHT ))"
  for i in $(seq $N); do
    line="$line m$i=$(grep -c '^accepted' "$RUN/logs$i/grind.log" 2>/dev/null || echo 0)"
  done
  echo "$line"
done
for i in $(seq $N); do wait "${MINER_PID[$i]}" 2>/dev/null || true; done
# Let the last shares propagate before stopping the nodes.
sleep 20

END_HEIGHT=$(cli 1 getblockcount)
echo "blocks: $START_HEIGHT -> $END_HEIGHT"
for i in $(seq $N); do
  echo "miner $i: $(grep -c '^accepted' "$RUN/logs$i/grind.log" || true) accepted, $(grep -c '^rejected' "$RUN/logs$i/grind.log" || true) rejected; knots height $(cli "$i" getblockcount)"
  grep -E '^rejected' "$RUN/logs$i/grind.log" | sort | uniq -c | sort -rn | head -3 || true
done
for i in $(seq $N); do
  kill -TERM -- "-${NODE_PID[$i]}" 2>/dev/null || true
done
for i in $(seq $N); do
  for _ in $(seq 30); do kill -0 "${NODE_PID[$i]}" 2>/dev/null || break; sleep 1; done
done
DBS=()
for i in $(seq $N); do DBS+=(--db "$RUN/shoal$i/store.db"); done
set +e
python3 "$(dirname "$0")/verify_pplns.py" --cli "$SHOAL_CLI" "${DBS[@]}" \
  --bitcoin-cli "$KNOTS_BIN/bitcoin-cli" --datadir "$RUN/node1" --network regtest | tee "$RUN/pplns.txt"
set -e
echo "RESULT blocks=$((END_HEIGHT - START_HEIGHT)) $(tail -1 "$RUN/pplns.txt")"
