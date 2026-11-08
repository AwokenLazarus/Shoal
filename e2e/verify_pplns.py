#!/usr/bin/env python3
"""Independent PPLNS check for a Shoal regtest run (test tool, not shipped).

For every sharechain share whose XBT block is on the main chain, recompute what its coinbase
must pay from the sharechain alone, and compare with the coinbase Knots accepted:

  window   = the share's parent and its ancestors back to genesis (at most 120,960 shares)
  weight   = per payout address, the sum of each share's integer difficulty
             (network max target // share target, rust-bitcoin's `Target::difficulty`)
  split    = coinbase value * weight // total, addresses in string order, the last address
             takes the rounding remainder (P2Poolv2's `append_proportional_distribution`)

Reads the sharechain with `p2poolv2_cli --db-path` (the node must be stopped) and the blocks
with `bitcoin-cli`. Shares with uncles are reported as not modelled (uncles come in M4).

Usage: verify_pplns.py --cli P2POOLV2_CLI --db STORE --bitcoin-cli BITCOIN_CLI --datadir DIR
                       [--network regtest]
"""
import argparse
import json
import subprocess
import sys

MAX_TARGET_BITS = {"regtest": 0x207FFFFF, "testnet4": 0x1D00FFFF, "main": 0x1D00FFFF}
PPLNS_WINDOW_SHARES = 120_960


def target(bits: int) -> int:
    exp, mant = bits >> 24, bits & 0xFFFFFF
    return mant * (256 ** (exp - 3)) if exp >= 3 else mant >> (8 * (3 - exp))


def run_json(cmd):
    return json.loads(subprocess.run(cmd, check=True, capture_output=True, text=True).stdout)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--cli", required=True)
    ap.add_argument("--db", required=True)
    ap.add_argument("--bitcoin-cli", required=True)
    ap.add_argument("--datadir", required=True)
    ap.add_argument("--network", default="regtest")
    a = ap.parse_args()
    max_target = target(MAX_TARGET_BITS[a.network])

    def difficulty(bits: int) -> int:
        return max_target // target(bits)

    def btc(*args):
        return run_json([a.bitcoin_cli, f"-datadir={a.datadir}", *args])

    info = run_json([a.cli, "--db-path", a.db, "info"])
    tip = info["chain_tip_height"]
    shares = run_json([a.cli, "--db-path", a.db, "shares", "--to", str(tip), "--num", str(tip + 1)])["shares"]
    by_hash = {s["blockhash"]: s for s in shares}

    checked = failed = skipped = 0
    for s in sorted(shares, key=lambda s: s["height"]):
        if s["height"] == 0:
            continue
        full = run_json([a.cli, "--db-path", a.db, "share", "--hash", s["blockhash"], "--full"])[0]
        block_hash = full["bitcoin_header"]["block_hash"]
        try:
            hdr = btc("getblockheader", block_hash)
        except subprocess.CalledProcessError:
            continue  # not a block (share only)
        if hdr.get("confirmations", -1) < 0:
            continue  # stale block
        if s["uncles"]:
            print(f"share {s['height']}: has uncles, not modelled (M4)")
            skipped += 1
            continue
        # Window: parent and ancestors.
        weights = {}
        cursor = by_hash.get(s["prev_blockhash"])
        n = 0
        while cursor is not None and n < PPLNS_WINDOW_SHARES:
            addr = cursor["miner_bitcoin_address"]
            weights[addr] = weights.get(addr, 0) + difficulty(int(cursor["bits"]))
            n += 1
            cursor = by_hash.get(cursor["prev_blockhash"])
        block = btc("getblock", block_hash, "2")
        paid = {}
        total = 0
        for o in block["tx"][0]["vout"]:
            sats = round(o["value"] * 1e8)
            if sats == 0:
                continue
            addr = o["scriptPubKey"].get("address", o["scriptPubKey"]["hex"])
            paid[addr] = paid.get(addr, 0) + sats
            total += sats
        total_w = sum(weights.values())
        expected = {}
        distributed = 0
        order = sorted(weights)
        for i, addr in enumerate(order):
            amt = total - distributed if i == len(order) - 1 else total * weights[addr] // total_w
            expected[addr] = amt
            distributed += amt
        checked += 1
        ok = expected == paid
        if not ok:
            failed += 1
        print(
            f"share {s['height']} -> block {block['height']} {block_hash[:16]}: window {n} shares, "
            f"{len(expected)} payees, {'OK' if ok else 'MISMATCH'}"
        )
        if not ok:
            print(f"  expected {expected}\n  paid     {paid}")
    print(f"PPLNS checked={checked} failed={failed} skipped={skipped}")
    return 1 if failed or checked == 0 else 0


if __name__ == "__main__":
    sys.exit(main())
