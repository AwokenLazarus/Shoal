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
  rule     = Shoal's payout rule (upstream's #652 design): a window share, or uncle, is paid
             only if its XBT parent is on the paying share's XBT chain (an ancestor of, or
             equal to, the paying share's XBT parent) or a child of a block on it. Ineligible
             shares keep their slot and weigh 0; a nephew gets no bonus for an ineligible uncle.
             The genesis share is always paid. XBT ancestry is read from Knots.
             `--no-payout-rule` checks upstream's rule instead.

With `--attacker-db`, that store is the attacking node's: it is left out of the tip check and
the main chain, and the payout to the addresses of shares on fabricated XBT parents (in any
store) is reported.

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
    ap.add_argument("--attacker-db", action="append", default=[])
    ap.add_argument("--no-payout-rule", action="store_true")
    ap.add_argument("--attack-address", default=None)
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

    known_block = {}
    xbt_headers = {}

    def xbt_header(h):
        """Knots' header for XBT block `h`, or None when Knots does not know it."""
        if h not in xbt_headers:
            try:
                xbt_headers[h] = btc("getblockheader", h)
            except subprocess.CalledProcessError:
                xbt_headers[h] = None
        return xbt_headers[h]

    def xbt_prev(h):
        return full(h)["bitcoin_header"]["prev_blockhash"]

    def xbt_parent_exists(h):
        """Whether the share's XBT parent block is one Knots knows (any branch)."""
        prev = xbt_prev(h)
        if prev not in known_block:
            known_block[prev] = xbt_header(prev) is not None
        return known_block[prev]

    paying_chains = {}

    def paying_chain(paying):
        """Hashes on the XBT chain ending at `paying`, back 1,000 blocks (Shoal's regtest depth)."""
        if paying not in paying_chains:
            chain = set()
            h = paying
            for _ in range(1001):
                hdr = xbt_header(h)
                if hdr is None:
                    break
                chain.add(h)
                if "previousblockhash" not in hdr:
                    break
                h = hdr["previousblockhash"]
            paying_chains[paying] = chain
        return paying_chains[paying]

    def eligible(share_hash, paying):
        """Shoal's payout rule for a window share or uncle, paid by a share on `paying`."""
        if a.no_payout_rule:
            return True
        f = full(share_hash)
        if f is None:
            return False
        if f.get("prev_share_blockhash", f.get("prev_blockhash", "")) in ("", "0" * 64):
            return True  # the genesis share
        prev = f["bitcoin_header"]["prev_blockhash"]
        chain = paying_chain(paying)
        if prev in chain:
            return True
        hdr = xbt_header(prev)
        return hdr is not None and hdr.get("previousblockhash") in chain

    # Addresses that mined on a fabricated XBT parent, in any store (attackers' included).
    dead_addresses = set()
    dead_shares = 0
    for d in a.db + a.attacker_db:
        top = cli(d, "info")["chain_tip_height"]
        for e in cli(d, "dag", "--to", str(top + 50), "--num", str(top + 100))["entries"]:
            if e.get("height", 1) == 0:
                continue
            h = e["blockhash"]
            if h not in full_cache:
                try:
                    full_cache[h] = cli(d, "share", "--hash", h, "--full")[0]
                except (subprocess.CalledProcessError, IndexError, json.JSONDecodeError):
                    continue
            if not xbt_parent_exists(h):
                dead_shares += 1
                dead_addresses.add(full_cache[h]["miner_bitcoin_address"])
    if dead_shares:
        print(f"shares on a fabricated XBT parent in any store: {dead_shares}")
    if dead_addresses:
        print(f"addresses that mined on a fabricated XBT parent: {sorted(dead_addresses)}")

    checked = failed = 0
    txs_by_miner = {}
    paid_to_attacker = 0
    paid_to_dead = 0
    paid_total = 0
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
        n_ineligible = 0
        paying = f["bitcoin_header"]["prev_blockhash"]
        for w in chain[skip:]:
            w_ok = eligible(w["blockhash"], paying)
            bonus = 0
            for u in w["uncles"]:
                n_uncles += 1
                if not eligible(uncle_hash(u), paying):
                    n_ineligible += 1
                    continue
                u_addr, u_diff = uncle_info(uncle_hash(u))
                weights[u_addr] = weights.get(u_addr, 0) + 9 * u_diff
                bonus += u_diff
            if not w_ok:
                n_ineligible += 1
                continue
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
        paid_total += total
        for addr, sats in paid.items():
            if addr in dead_addresses:
                paid_to_dead += sats
            if addr == a.attack_address:
                paid_to_attacker += sats
        ok = expected == paid
        if not ok:
            failed += 1
        all_payees.update(paid)
        row = txs_by_miner.setdefault(s["miner_bitcoin_address"], [0, 0, 0])
        row[0] += 1
        row[1] += len(block["tx"]) - 1
        row[2] += total
        print(
            f"share {s['height']} -> block {block['height']} {block_hash[:16]}: "
            f"window {len(chain[skip:])} shares (skip {skip}), {n_uncles} uncles, "
            f"{n_ineligible} ineligible, {len(block['tx']) - 1} txs, "
            f"{len(expected)} payees, {'OK' if ok else 'MISMATCH'}"
        )
        if not ok:
            print(f"  expected {expected}\n  paid     {paid}")
    print(f"payees seen across blocks: {len(all_payees)}")
    for addr, (n_blocks, n_txs, value) in sorted(txs_by_miner.items()):
        print(f"TEMPLATES miner {addr}: {n_blocks} blocks, {n_txs} txs, {value} sats paid out")
    dead_main = [h for h in by_hash if by_hash[h]["height"] > 0 and not xbt_parent_exists(h)]
    print(
        f"DEAD-TEMPLATE main-chain shares on a fabricated XBT parent: {len(dead_main)}; "
        f"paid to their miners by real blocks: {paid_to_dead} of {paid_total} sats "
        f"({100.0 * paid_to_dead / max(paid_total, 1):.1f}%)"
    )

    if a.attack_address:
        print(
            f"ATTACKER address {a.attack_address} paid by real blocks: {paid_to_attacker} of "
            f"{paid_total} sats ({100.0 * paid_to_attacker / max(paid_total, 1):.1f}%)"
        )

    # Withholding statistic, from public data only: per miner, main-chain shares whose XBT
    # header meets the XBT block target (on regtest, every share) and how many of those are
    # blocks on the XBT chain.
    per_miner = {}
    for h, x in by_hash.items():
        if x["height"] == 0:
            continue
        fx = full(h)
        bh = fx["bitcoin_header"]
        bits = int(bh["bits"], 16) if isinstance(bh["bits"], str) else int(bh["bits"])
        meets = int(bh["block_hash"], 16) <= target(bits)
        hdr = xbt_header(bh["block_hash"])
        found = hdr is not None and hdr.get("confirmations", -1) >= 0
        row = per_miner.setdefault(x["miner_bitcoin_address"], [0, 0])
        row[0] += 1 if meets else 0
        row[1] += 1 if found else 0
    for addr, (meets, found) in sorted(per_miner.items()):
        flag = "  <- WITHHOLDING?" if meets >= 5 and found == 0 else ""
        print(f"BLOCKS miner {addr}: {meets} shares met the block target, {found} on the XBT chain{flag}")

    # Copied shares (Monero P2Pool GHSA-fm6j-gf38-p925): one XBT header (one proof of work)
    # behind more than one share, in any store.
    header_owner = {}
    dup = 0
    for d in a.db + a.attacker_db:
        for e in cli(d, "dag", "--to", str(tip + 50), "--num", str(tip + 100))["entries"]:
            fx = full(e["blockhash"])
            if fx is None or e.get("height", 1) == 0:
                continue
            xh = fx["bitcoin_header"]["block_hash"]
            owner = header_owner.setdefault(xh, e["blockhash"])
            if owner != e["blockhash"]:
                dup += 1
    print(f"COPIES shares sharing one XBT header with another share: {dup}")

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
