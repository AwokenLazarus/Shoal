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
# M3 (no operator): KILL_NODE=<1..3> kills that Shoal node and its miner with SIGKILL at
# KILL_AT seconds (default 600), keeps it down DOWN_SECS (default 300), then restarts it on the
# same store; it must resync from its peers before serving work again. The other two keep mining.
# Every node dials every other, so any node can die, including node 1, which started the chain.
#
# M4 (latency): NETEM_MS=<ms> delays the share p2p traffic between the nodes (ports 6901-6903
# only, both directions, so RTT is about twice that) with NETEM_JITTER_MS jitter, using tc netem
# on lo through sudo. Other traffic on the host is untouched; the qdisc is removed on exit, and
# the run refuses to start if lo already has a root qdisc of its own.
# STRATUM_DIFF / STRATUM_MAX_DIFF set the session difficulty in pool units (regtest: 1/1024 of
# difficulty 1 each, see the fork's xbt_params.rs).
#
# M5 (attacks): ATTACK_NODE=<1..3> runs that node from ATTACK_BIN, e.g. a fork build with
# `--features shoal-attack-dead-template` (every job on a fabricated XBT parent). The checker
# reports what honest blocks paid for such shares. With a `--features shoal-attack` build,
# ATTACK_MODE (withhold, wrong-split, copy, private-fork) sets the node's SHOAL_ATTACK mode; it
# gets its own SHOAL_ATTACK_ADDRESS, and ATTACK_THREADS miner threads (default THREADS).
# private-fork reads SHOAL_ATTACK_PRIVATE_FROM / _UNTIL (seconds after the node starts).
#
# MIXED_POLICY=1: node 3's Knots builds templates with -blockmintxfee=0.0005 (50 sat/vB) while
# nodes 1 and 2 keep the default, and node 1's wallet sends a transaction every 5 s,
# alternating 1 and 100 sat/vB. Templates differ; every block must still pay the window. The
# long coinbase maturity starts at height 5,000 in this mode, so the feed's coins (heights
# 121-140) mature after 101 blocks; the run tests template policy, not maturity.
#
# Env: KNOTS_BIN, SHOAL_NODE, SHOAL_CLI, GRIND (as m1), SECS (mining time, default 1800),
#      THREADS (per miner, default 2), LOG_LEVEL, KILL_NODE, KILL_AT, DOWN_SECS,
#      NETEM_MS, NETEM_JITTER_MS, STRATUM_DIFF (default 1024), STRATUM_MAX_DIFF
set -euo pipefail
KNOTS_BIN=${KNOTS_BIN:-$HOME/xbt-lab/prefix/bin}
SHOAL_NODE=${SHOAL_NODE:-$HOME/shoal/p2poolv2/target/debug/p2poolv2}
SHOAL_CLI=${SHOAL_CLI:-$HOME/shoal/p2poolv2/target/debug/p2poolv2_cli}
GRIND=${GRIND:-$HOME/shoal/shoal/target/release/shoal-grind}
SECS=${SECS:-1800}
THREADS=${THREADS:-2}
KILL_NODE=${KILL_NODE:-}
KILL_AT=${KILL_AT:-600}
DOWN_SECS=${DOWN_SECS:-300}
NETEM_MS=${NETEM_MS:-}
NETEM_JITTER_MS=${NETEM_JITTER_MS:-0}
STRATUM_DIFF=${STRATUM_DIFF:-1024}
STRATUM_MAX_DIFF=${STRATUM_MAX_DIFF:-}
ATTACK_NODE=${ATTACK_NODE:-}
ATTACK_BIN=${ATTACK_BIN:-}
ATTACK_MODE=${ATTACK_MODE:-}
ATTACK_THREADS=${ATTACK_THREADS:-}
ATTACK_ADDR=""
MIXED_POLICY=${MIXED_POLICY:-}
# ASIC_NODE=<2|3>: that node serves Stratum on the LAN (0.0.0.0) for a real ASIC instead of a
# CPU miner, at ASIC_DIFF and never below it (pool units, 1/1024 of difficulty 1 on regtest;
# default 4,194,304 = difficulty 4096, about one submission per 4 s at 4.2 TH/s). The script prints the URL, user and password to set on the miner.
ASIC_NODE=${ASIC_NODE:-}
ASIC_DIFF=${ASIC_DIFF:-4194304}
LAN_IP=${LAN_IP:-$(hostname -I | awk '{print $1}')}
if [ -n "$ATTACK_NODE" ] && [ ! -x "$ATTACK_BIN" ]; then echo "FAIL: ATTACK_BIN not executable"; exit 1; fi
RUN=${RUN:-$HOME/shoal/e2e/m2-$(date -u +%Y%m%dT%H%M%SZ)}
N=3
rpc_port() { echo $((38510 + $1)); }
p2p_port() { echo $((38500 + $1)); }
zmq_port() { echo $((28500 + $1)); }
stratum_port() { echo $((33500 + $1)); }
share_port() { echo $((6900 + $1)); }
api_port() { echo $((46900 + $1)); }

PIDS=()
NETEM_ON=0
cleanup() {
  set +e
  if [ "$NETEM_ON" = 1 ]; then sudo -n tc qdisc del dev lo root 2>/dev/null && echo "netem removed"; fi
  for p in "${PIDS[@]}"; do kill -TERM -- "-$p" 2>/dev/null || kill "$p" 2>/dev/null; done
  sleep 3
  for p in "${PIDS[@]}"; do kill -KILL -- "-$p" 2>/dev/null; done
  for i in $(seq $N); do "$KNOTS_BIN/bitcoin-cli" -datadir="$RUN/node$i" stop >/dev/null 2>&1; done
  sleep 2
  echo "cleanup done; run dir $RUN"
}
trap cleanup EXIT
trap 'exit 130' INT; trap 'exit 143' TERM; trap 'exit 129' HUP

if [ -n "$NETEM_MS" ]; then
  if tc qdisc show dev lo | grep -q "qdisc prio 1:\|qdisc netem"; then
    echo "FAIL: lo already has a prio/netem root qdisc; not touching it"; exit 1
  fi
  if tc qdisc show dev lo | grep -v "noqueue" | grep -q "root"; then
    echo "FAIL: lo has a non-default root qdisc; not touching it"; exit 1
  fi
  # Bands 1-3 carry everything as before; band 4 (netem) only the share p2p ports.
  sudo -n tc qdisc add dev lo root handle 1: prio bands 4 priomap 1 2 2 2 1 2 0 0 1 1 1 1 1 1 1 1
  NETEM_ON=1
  sudo -n tc qdisc add dev lo parent 1:4 handle 40: netem delay "${NETEM_MS}ms" "${NETEM_JITTER_MS}ms"
  for port in 6901 6902 6903; do
    sudo -n tc filter add dev lo protocol ip parent 1:0 prio 1 u32 match ip dport "$port" 0xffff flowid 1:4
    sudo -n tc filter add dev lo protocol ip parent 1:0 prio 1 u32 match ip sport "$port" 0xffff flowid 1:4
  done
  echo "netem: ${NETEM_MS}ms ± ${NETEM_JITTER_MS}ms one way on share p2p ports 6901-6903"
fi

cli() { local i=$1; shift; "$KNOTS_BIN/bitcoin-cli" -datadir="$RUN/node$i" "$@"; }
accepted() { local n; n=$(grep -c '^accepted' "$RUN/logs$1/grind.log" 2>/dev/null) || true; echo "${n:-0}"; }

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
$(if [ -n "$MIXED_POLICY" ]; then echo "testcoinbasematuritylong=5000:5000:5101"; else echo "testcoinbasematuritylong=200:200:6680"; fi)
fallbackfee=0.0001
$([ -n "$MIXED_POLICY" ] && [ "$i" = 3 ] && echo "blockmintxfee=0.0005")
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
PREMINE=120
cli 1 generatetodescriptor 120 'raw(51)' >/dev/null
if [ -n "$MIXED_POLICY" ]; then
  # Coins for the transaction feed: 20 blocks to node 1's wallet, matured by 200 more.
  FEED_ADDR=$(cli 1 -rpcwallet=shoal1 getnewaddress "" bech32)
  cli 1 generatetoaddress 20 "$FEED_ADDR" >/dev/null
  cli 1 generatetodescriptor 111 'raw(51)' >/dev/null
  PREMINE=251
fi
for i in $(seq 2 $N); do
  for _ in $(seq 120); do [ "$(cli "$i" getblockcount)" = $PREMINE ] && break; sleep 0.5; done
  [ "$(cli "$i" getblockcount)" = $PREMINE ] || { echo "FAIL: knots node $i did not sync"; exit 1; }
done
echo "knots: $N nodes at height $PREMINE"
[ -n "$MIXED_POLICY" ] && echo "mixed policy: node 3 blockmintxfee=0.0005; feed balance $(cli 1 -rpcwallet=shoal1 getbalance)"

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
$(if [ "$i" = "$ASIC_NODE" ]; then echo 'hostname = "0.0.0.0"'; echo "start_difficulty = $ASIC_DIFF"
  echo "minimum_difficulty = $ASIC_DIFF"; else
  echo 'hostname = "127.0.0.1"'; echo "start_difficulty = $STRATUM_DIFF"; echo "minimum_difficulty = 1"
  [ -n "$STRATUM_MAX_DIFF" ] && echo "maximum_difficulty = $STRATUM_MAX_DIFF"; fi)
port = $(stratum_port "$i")
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

declare -A NODE_PID MINER_PID KILL_ACC
start_node() {
  local i=$1 bin=$SHOAL_NODE
  local attack_env=()
  if [ "$i" = "$ATTACK_NODE" ]; then
    bin=$ATTACK_BIN; echo "node $i runs the attack build $ATTACK_BIN${ATTACK_MODE:+ mode $ATTACK_MODE}"
    if [ -n "$ATTACK_MODE" ]; then
      [ -n "$ATTACK_ADDR" ] || ATTACK_ADDR=$(cli "$i" -rpcwallet="shoal$i" getnewaddress "" bech32)
      echo "attacker address $ATTACK_ADDR"
      attack_env=(SHOAL_ATTACK="$ATTACK_MODE" SHOAL_ATTACK_ADDRESS="$ATTACK_ADDR")
    fi
  fi
  setsid env "${attack_env[@]}" timeout -k 10 $((SECS + 900)) "$bin" --config "$RUN/shoal$i/config.toml" \
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
  local i=$1 secs=$2 threads=$THREADS
  if [ "$i" = "$ASIC_NODE" ]; then
    echo "ASIC: set the miner to stratum+tcp://$LAN_IP:$(stratum_port "$i") user ${PAYOUT[$i]}.asic pass p2p=${SHARE_ADDR[$i]}"
    return 0
  fi
  if [ "$i" = "$ATTACK_NODE" ] && [ -n "$ATTACK_THREADS" ]; then threads=$ATTACK_THREADS; fi
  setsid timeout -k 5 "$secs" "$GRIND" --port "$(stratum_port "$i")" --user "${PAYOUT[$i]}.cpu$i" \
    --pass "p2p=${SHARE_ADDR[$i]}" --threads "$threads" --secs "$secs" >>"$RUN/logs$i/grind.log" 2>&1 &
  MINER_PID[$i]=$!; PIDS+=($!)
}
dial_others() {  # TOML list of every other node's share p2p address
  local i=$1 j out=""
  for j in $(seq $N); do
    [ "$j" = "$i" ] && continue
    out="$out${out:+, }\"/ip4/127.0.0.1/tcp/$(share_port "$j")\""
  done
  echo "$out"
}

START_HEIGHT=$(cli 1 getblockcount)
T0=$(date +%s)
# A fresh ASERT anchor: the sharechain's genesis time is the run's start (same for every node
# and every restart; see the fork's xbt_params::REGTEST_GENESIS_TIME_ENV).
export SHOAL_REGTEST_GENESIS_TIME=${SHOAL_REGTEST_GENESIS_TIME:-$T0}
# Start ASERT near a 10 s share: difficulty 1/8 (about 2^29 hashes) suits three 2-thread CPU
# miners (~50 MH/s together). Unset (empty) keeps the fork's single-miner default.
export SHOAL_REGTEST_GENESIS_BITS=${SHOAL_REGTEST_GENESIS_BITS-1d07ffff}
[ -z "$SHOAL_REGTEST_GENESIS_BITS" ] && unset SHOAL_REGTEST_GENESIS_BITS
END_AT=$((T0 + SECS))
# Node 1 starts the sharechain: nobody to sync from.
write_config 1 false "$(dial_others 1)"
start_node 1
wait_stratum 1 90
echo "node 1 stratum up"
start_miner 1 "$SECS"
# Wait for node 1's first share so the chain tip is current, then let nodes 2 and 3 join and
# sync before they serve work (upstream's default wait_for_chain_sync = true).
for _ in $(seq 600); do
  [ "$(accepted 1)" -ge 1 ] 2>/dev/null && break
  sleep 1
done
echo "node 1 first share after $(( $(date +%s) - T0 )) s"
for i in $(seq 2 $N); do
  write_config "$i" true "$(dial_others "$i")"
  start_node "$i"
done
for i in $(seq 2 $N); do
  wait_stratum "$i" 300
  echo "node $i synced, stratum up after $(( $(date +%s) - T0 )) s"
  start_miner "$i" $(( END_AT - $(date +%s) ))
done

progress() {
  local line="t+$(( $(date +%s) - T0 ))s blocks=$(( $(cli 1 getblockcount) - START_HEIGHT ))" i
  for i in $(seq $N); do line="$line m$i=$(accepted "$i")"; done
  echo "$line${1:+ $1}"
}
KILLED=0 RESTARTED=0 NEXT_PROGRESS=$((T0 + 300)) FEED_N=0
while [ "$(date +%s)" -lt "$END_AT" ]; do
  sleep 5
  now=$(date +%s)
  if [ -n "$MIXED_POLICY" ]; then
    FEED_N=$((FEED_N + 1)); rate=$([ $((FEED_N % 2)) = 0 ] && echo 100 || echo 1)
    cli 1 -rpcwallet=shoal1 -named sendtoaddress address="$FEED_ADDR" amount=0.01 fee_rate=$rate >/dev/null 2>&1 \
      || echo "feed tx $FEED_N failed"
  fi
  if [ -n "$KILL_NODE" ] && [ "$KILLED" = 0 ] && [ "$now" -ge $((T0 + KILL_AT)) ]; then
    kill -KILL -- "-${MINER_PID[$KILL_NODE]}" 2>/dev/null || true
    kill -KILL -- "-${NODE_PID[$KILL_NODE]}" 2>/dev/null || true
    KILLED=$now KILL_HEIGHT=$(cli 1 getblockcount)
    for i in $(seq $N); do KILL_ACC[$i]=$(accepted "$i"); done
    progress "KILLED node $KILL_NODE (SIGKILL, node and miner)"
  fi
  if [ "$KILLED" != 0 ] && [ "$RESTARTED" = 0 ] && [ "$now" -ge $((KILLED + DOWN_SECS)) ]; then
    DOWN_BLOCKS=$(( $(cli 1 getblockcount) - KILL_HEIGHT ))
    DOWN_SHARES=""
    for i in $(seq $N); do DOWN_SHARES="$DOWN_SHARES m$i+$(( $(accepted "$i") - KILL_ACC[$i] ))"; done
    progress "while node $KILL_NODE was down: $DOWN_BLOCKS blocks,$DOWN_SHARES shares"
    # Same store, upstream's default: sync from peers before serving work.
    write_config "$KILL_NODE" true "$(dial_others "$KILL_NODE")"
    start_node "$KILL_NODE"
    if wait_stratum "$KILL_NODE" 300; then
      RESTARTED=$(date +%s)
      progress "node $KILL_NODE restarted, resynced and serving after $((RESTARTED - KILLED - DOWN_SECS)) s"
      start_miner "$KILL_NODE" $(( END_AT - RESTARTED ))
    else
      RESTARTED=-1
      echo "FAIL: node $KILL_NODE did not come back"
    fi
  fi
  if [ "$now" -ge "$NEXT_PROGRESS" ]; then progress; NEXT_PROGRESS=$((NEXT_PROGRESS + 300)); fi
done
for i in $(seq $N); do [ "$i" = "$ASIC_NODE" ] || wait "${MINER_PID[$i]}" 2>/dev/null || true; done
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
for i in $(seq $N); do
  if [ "$i" = "$ATTACK_NODE" ]; then DBS+=(--attacker-db "$RUN/shoal$i/store.db")
  else DBS+=(--db "$RUN/shoal$i/store.db"); fi
done
set +e
[ -n "$ATTACK_ADDR" ] && DBS+=(--attack-address "$ATTACK_ADDR")
ALARMS=""
for i in $(seq $N); do
  [ "$i" = "$ATTACK_NODE" ] && continue
  n=$(cat "$RUN/logs$i"/shoal.log* 2>/dev/null | grep -c "SHARECHAIN REORG ALARM" || true)
  echo "node $i reorg alarms: $n"
  ALARMS="$ALARMS node$i=$n"
done
python3 "$(dirname "$0")/verify_pplns.py" --cli "$SHOAL_CLI" "${DBS[@]}" \
  --bitcoin-cli "$KNOTS_BIN/bitcoin-cli" --datadir "$RUN/node1" --network regtest | tee "$RUN/pplns.txt"
set -e
M3=""
if [ -n "$KILL_NODE" ]; then
  M3=" killed=node$KILL_NODE down_blocks=${DOWN_BLOCKS:-?} restarted=$([ "$RESTARTED" -gt 0 ] && echo yes || echo NO)"
fi
echo "RESULT blocks=$((END_HEIGHT - START_HEIGHT)) $(tail -1 "$RUN/pplns.txt")$M3${ATTACK_MODE:+ attack=$ATTACK_MODE} reorg_alarms:$ALARMS"
