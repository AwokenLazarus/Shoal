#!/usr/bin/env python3
"""Independent PPLNS check for a Shoal regtest run (test tool, not shipped).

For every sharechain share whose XBT block is on the main chain, recompute what its coinbase
must pay from the sharechain alone, and compare with the coinbase Knots accepted:

  window   = from the share's parent back: skip the shares not yet mature for it
             (min(maturity - 1, height - maturity)), then take `window` shares
             (regtest: 12 and 4, Shoal's regtest params; other networks 120,960 and 6,048)
  weight   = per payout address, 10 x each window share's integer difficulty
             (network max target // share target, rust-bitcoin's `Target::difficulty`);
             each uncle a window share includes adds 9 x the uncle's difficulty to the
             uncle's miner and 1 x to the including share's miner
  split    = coinbase value * weight // total, addresses in string order, the last address
             takes the rounding remainder (P2Poolv2's `append_proportional_distribution`)

Reads the sharechain with `p2poolv2_cli --db-path` (the node must be stopped) and the blocks
with `bitcoin-cli`. Several stores (one per node) can be given: their tips must match, and
every share's block is checked once.

Usage: verify_pplns.py --cli P2POOLV2_CLI --db STORE [--db STORE ...]
                       --bitcoin-cli BITCOIN_CLI --datadir DIR [--network regtest]
"""
import argparse
import json
import subprocess
import sys

MAX_TARGET_BITS = {"regtest": 0x207FFFFF, "testnet4": 0x1D00FFFF, "main": 0x1D00FFFF}
WINDOW = {"regtest": (12, 4)}  # (window shares, maturity); see p2poolv2 xbt_params.rs
DEFAULT_WINDOW = (120_960, 6_048)


def target(bits: int) -> int:
    exp, mant = bits >> 24, bits & 0xFFFFFF
    return mant * (256 ** (exp - 3)) if exp >= 3 else mant >> (8 * (3 - exp))


def uncle_hash(u):
    """An uncle as `shares` lists it: an object with its blockhash (or a bare hash)."""
    return u["blockhash"] if isinstance(u, dict) else u


def run_json(cmd):
    return json.loads(subprocess.run(cmd, check=True, capture_output=True, text=True).stdout)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--cli", required=True)
    ap.add_argument("--db", required=True, action="append")
    ap.add_argument("--bitcoin-cli", required=True)
    ap.add_argument("--datadir", required=True)
    ap.add_argument("--network", default="regtest")
    a = ap.parse_args()
    max_target = target(MAX_TARGET_BITS[a.network])
    window_shares, maturity = WINDOW.get(a.network, DEFAULT_WINDOW)

    def difficulty(bits: int) -> int:
        return max_target // target(bits)

    def btc(*args):
        return run_json([a.bitcoin_cli, f"-datadir={a.datadir}", *args])

    def cli(db, *args):
        return run_json([a.cli, "--db-path", db, *args])

    # Tips must agree across stores.
    tips = {}
    for db in a.db:
        info = cli(db, "info")
        tips[db] = (info["chain_tip_height"], info.get("chain_tip_blockhash", info.get("chain_tip")))
    distinct = set(tips.values())
    for db, tip in tips.items():
        print(f"store {db}: tip height {tip[0]} {str(tip[1])[:16]}")
    tips_ok = len(distinct) == 1
    print(f"TIPS {'identical' if tips_ok else 'DIFFER'}")

    db = a.db[0]
    tip = tips[db][0]
    shares = cli(db, "shares", "--to", str(tip), "--num", str(tip + 1))["shares"]
    by_hash = {s["blockhash"]: s for s in shares}
    full_cache = {}

    def full(h):
        if h not in full_cache:
            for d in a.db:  # an uncle may be stored on some nodes only
                try:
                    full_cache[h] = cli(d, "share", "--hash", h, "--full")[0]
                    break
                except (subprocess.CalledProcessError, IndexError, json.JSONDecodeError):
                    continue
        return full_cache.get(h)

    def uncle_info(h):
        f = full(h)
        if f is None:
            raise SystemExit(f"uncle {h} not found in any store")
        return f["miner_bitcoin_address"], difficulty(int(f["bits"], 16))

    checked = failed = 0
    all_payees = set()
    for s in sorted(shares, key=lambda s: s["height"]):
        if s["height"] == 0:
            continue
        f = full(s["blockhash"])
        block_hash = f["bitcoin_header"]["block_hash"]
        try:
            hdr = btc("getblockheader", block_hash)
        except subprocess.CalledProcessError:
            continue  # not a block (share only)
        if hdr.get("confirmations", -1) < 0:
            continue  # stale block
        # Window: from the parent back, skip the immature shares, then take the window.
        skip = min(maturity - 1, max(s["height"] - maturity, 0))
        chain = []
        cursor = by_hash.get(s["prev_blockhash"])
        while cursor is not None and len(chain) < skip + window_shares:
            chain.append(cursor)
            cursor = by_hash.get(cursor["prev_blockhash"])
        weights = {}
        n_uncles = 0
        for w in chain[skip:]:
            bonus = 0
            for u in w["uncles"]:
                u_addr, u_diff = uncle_info(uncle_hash(u))
                weights[u_addr] = weights.get(u_addr, 0) + 9 * u_diff
                bonus += u_diff
                n_uncles += 1
            addr = w["miner_bitcoin_address"]
            weights[addr] = weights.get(addr, 0) + 10 * difficulty(int(w["bits"])) + bonus
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
        all_payees.update(paid)
        print(
            f"share {s['height']} -> block {block['height']} {block_hash[:16]}: "
            f"window {len(chain[skip:])} shares (skip {skip}), {n_uncles} uncles, "
            f"{len(expected)} payees, {'OK' if ok else 'MISMATCH'}"
        )
        if not ok:
            print(f"  expected {expected}\n  paid     {paid}")
    print(f"payees seen across blocks: {len(all_payees)}")

    # Share DAG across all stores: main chain, uncles (referenced by a main-chain share) and
    # orphans (neither). Work = integer difficulty from each share's bits.
    entries = {}
    for d in a.db:
        for e in cli(d, "dag", "--to", str(tip + 50), "--num", str(tip + 100))["entries"]:
            entries.setdefault(e["blockhash"], e)
    main = {h for h in by_hash}
    uncles = {uncle_hash(u) for h in main for u in by_hash[h]["uncles"]}
    orphans = [h for h in entries if h not in main and h not in uncles]
    invalid = [h for h, e in entries.items() if e.get("validation_status") == "Invalid"]

    def work(h):
        f = full(h)
        return difficulty(int(f["bits"], 16)) if f else 0

    w_main = sum(work(h) for h in main if by_hash[h]["height"] > 0)
    w_uncle = sum(work(h) for h in uncles)
    w_orphan = sum(work(h) for h in orphans)
    no_data = [h for h in orphans if full(h) is None]
    if no_data:
        print(f"orphans without a stored header (work unknown, counted as 0): {len(no_data)}")
    for h in orphans[:5]:
        e = entries[h]
        print(f"  orphan h={e['height']} {h[:16]} status={e.get('validation_status')} "
              f"chain={e.get('chain')} data={e.get('has_block_data')} work={work(h)}")
    w_all = w_main + w_uncle + w_orphan
    lost = 100.0 * w_orphan / w_all if w_all else 0.0
    print(
        f"DAG shares={len(entries)} main={len(main) - 1} uncles={len(uncles)} orphans={len(orphans)} "
        f"invalid={len(invalid)} uncle_rate={100.0 * len(uncles) / max(len(entries) - 1, 1):.1f}% "
        f"work_lost={lost:.2f}% orphan_rate={100.0 * len(orphans) / max(len(entries) - 1, 1):.1f}%"
    )
    print(f"PPLNS checked={checked} failed={failed} tips={'same' if tips_ok else 'differ'}")
    return 1 if failed or checked == 0 or not tips_ok else 0


if __name__ == "__main__":
    sys.exit(main())
