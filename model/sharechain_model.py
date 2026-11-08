#!/usr/bin/env python3
"""XBT P2Pool feasibility model (phase 1). Pure arithmetic, no network, no node.

Inputs
  - model/lazarus_sizes_2026-11-08.json: TH/s per payout address online at Lazarus on
    2026-11-08 (201 addresses, 14.7 PH/s). Used as the *shape* of a realistic XBT miner
    population; a p2pool of P PH/s is assumed to have the same mix, scaled in count.
  - Network 50.8 PH/s, difficulty 6.33e9 (diff-1 units = 2^32 hashes), 3.125 XBT subsidy.

Questions it answers
  1. How lumpy is a small miner's income on a sharechain, per day and per month, versus solo?
     (What matters for income is shares per month, not "has a share in this window":
     PPLNS pays every share for every block found while it is in the window.)
  2. How many distinct payees does a coinbase need, and does it fit
     (stock DATUM gateway coinbase buffer ~16.9 KB; Prime's coinbaser cap 512 outputs;
     RDTS block weight 800k WU)?
  3. Stale / uncle rates and relay bandwidth for share intervals at XBT block times.
"""
import json, math, os

HERE = os.path.dirname(os.path.abspath(__file__))
SIZES = json.load(open(os.path.join(HERE, "lazarus_sizes_2026-11-08.json")))["th_s"]
LAZ_PHS = sum(SIZES) / 1000

NET_PHS = 50.8
DIFF = 6.333820358e9          # diff-1 units
HASHES_PER_DIFF1 = 2 ** 32
BLOCK_S = DIFF * HASHES_PER_DIFF1 / (NET_PHS * 1e15)   # ~535 s at today's hashrate
MONTH_S = 30 * 86400
SUBSIDY_SATS = 312_500_000

GW_COINBASE_BYTES = ((1024 + 32768) >> 1) + 64   # datum_gateway MAX_COINBASE_TXN_SIZE_BYTES
PRIME_MAX_OUTPUTS = 512
RDTS_WEIGHT = 800_000
OUT_P2TR, OUT_P2WPKH = 43, 31                    # bytes per output (8 value + 1 len + script)
NTA_ATTEST_OUT = 8 + 1 + 70                      # NTA draft: one 70-byte OP_RETURN per payee
CB_FIXED = 150                                   # version, input, scriptSig, commitment, locktime

SHARE_BYTES = 164 + 32 * 12 + 250 + 120          # header + coinbase branch + coinbase minus outputs + sharechain fields

REP = [0.5, 2.3, 10, 40, 150, 500]               # representative miner sizes, TH/s


def pct(x):
    return f"{100 * x:5.1f}%"


def cv_month(h_th, pool_phs, T):
    """Coefficient of variation of one month's income: own share count (Poisson) and pool luck."""
    shares = (h_th / 1000 / pool_phs) * MONTH_S / T
    blocks = MONTH_S / (BLOCK_S * NET_PHS / pool_phs)
    return math.sqrt(1 / shares + 1 / blocks), shares


def solo_cv_month(h_th):
    blocks = MONTH_S / BLOCK_S * (h_th / 1000 / NET_PHS)
    return blocks


def distinct_payees(pool_phs, window_shares, sizes=SIZES, base_phs=LAZ_PHS):
    """Expected distinct addresses with >=1 share in a window of `window_shares` shares,
    for a pool of pool_phs whose miner mix is `sizes` scaled in count."""
    f = pool_phs / base_phs
    return f * sum(1 - (1 - (h / 1000) / pool_phs) ** window_shares for h in sizes)


def main():
    print(f"network {NET_PHS} PH/s, block every {BLOCK_S:.0f} s; sample population {len(SIZES)} addresses, {LAZ_PHS:.1f} PH/s")
    small = [h for h in SIZES if h < 20]
    print(f"  addresses <20 TH/s: {len(small)} of {len(SIZES)}, {sum(small)/1000:.2f} PH/s ({sum(small)/1000/LAZ_PHS:.0%} of the hash)\n")

    print("1. Income noise per month (CV), sharechain at 10 s shares vs solo")
    print(f"{'pool PH/s':>9} {'block every':>11} " + " ".join(f"{str(h)+' TH':>12}" for h in REP))
    for P in (1, 5, 15):
        cells = []
        for h in REP:
            cv, n = cv_month(h, P, 10)
            cells.append(f"{pct(cv)} ({n:4.0f})" if n < 9999 else f"{pct(cv)}")
        print(f"{P:>9} {BLOCK_S*NET_PHS/P/3600:9.1f} h " + " ".join(f"{c:>12}" for c in cells))
    print(f"{'solo':>9} {'':>11} " + " ".join(f"{solo_cv_month(h):8.2f} blk" for h in REP))
    print("   (n) = own shares per month. Solo row = expected blocks per month.\n")

    # mini chain: small miners only, own 10 s difficulty
    mini_phs = sum(small) / 1000
    print(f"   mini chain (only <20 TH/s miners, same mix as sample): {mini_phs:.2f} PH/s per 14.7 PH/s of pool")
    for scale in (1/3, 1):
        mp = mini_phs * scale
        cells = []
        for h in REP[:3]:
            cv, n = cv_month(h, mp, 10)
            cells.append(f"{h} TH {pct(cv)} ({n:.0f}/mo)")
        print(f"   mini at {mp:.2f} PH/s: " + ", ".join(cells))
    print()

    print("2. Distinct payees per coinbase (PPLNS window in shares) and whether they fit")
    print(f"   limits: stock gateway coinbase {GW_COINBASE_BYTES} B -> {(GW_COINBASE_BYTES-CB_FIXED)//OUT_P2TR} P2TR / "
          f"{(GW_COINBASE_BYTES-CB_FIXED)//OUT_P2WPKH} P2WPKH outputs; Prime coinbaser {PRIME_MAX_OUTPUTS}; "
          f"RDTS weight {RDTS_WEIGHT} WU -> {RDTS_WEIGHT//4//OUT_P2TR} P2TR")
    print(f"{'pool PH/s':>9} {'window':>7} {'span':>7} {'payees':>7} {'cb KB P2TR':>10} {'% blk wt':>8} {'w/ NTA KB':>9}")
    for P in (1, 5, 15):
        for W in (720, 2160, 8640):
            n = distinct_payees(P, W)
            kb = (CB_FIXED + n * OUT_P2TR) / 1000
            nta = (CB_FIXED + n * (OUT_P2TR + NTA_ATTEST_OUT)) / 1000
            print(f"{P:>9} {W:>7} {W*10/3600:5.1f} h {n:7.0f} {kb:10.1f} {kb*4000/RDTS_WEIGHT*100:7.1f}% {nta:9.1f}")
    print()

    print("3. Stale shares and relay at XBT block times (linear chain; uncles recover most of it)")
    print(f"{'interval':>8} {'stale@0.3s':>10} {'stale@1.0s':>10} {'stale@2.0s':>10} {'MB/day/peer':>11}")
    for T in (30, 10, 5, 2, 1):
        s = [1 - math.exp(-L / T) for L in (0.3, 1.0, 2.0)]
        mb = 86400 / T * SHARE_BYTES / 1e6
        print(f"{T:>6} s " + " ".join(f"{pct(x):>10}" for x in s) + f" {mb:11.1f}")
    print(f"   share size assumed {SHARE_BYTES} B (outputs are recomputed, not sent)")
    print(f"   at 10 s shares a pool block carries ~{BLOCK_S*NET_PHS/5/10:.0f} shares at 5 PH/s, ~{BLOCK_S*NET_PHS/15/10:.0f} at 15 PH/s")


if __name__ == "__main__":
    main()
