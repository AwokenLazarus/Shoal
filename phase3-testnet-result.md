# Shoal — phase 3 result (XBT testnet4 trial)

2026-11-13. Phase 3 asked: does Shoal work on a real, public XBT network, with a real ASIC,
for days, and do other XBT nodes accept its blocks? Answer: **yes**. Over 77 hours one Shoal
node and one ASIC found 200 testnet4 blocks, other miners built on them, and an independent
checker rebuilt every coinbase on the final sharechain to the satoshi, including blocks that
split the reward between two addresses.

## Verdict
**Go** for the remaining phase 3 work: a second independent node, Shoal DNS seeds, then a
public testnet4 alpha. Mainnet stays a separate decision.

## Setup
- One Shoal node (the fork's release build) on a LAN VM, beside Knots-BLAKE2b synced to XBT
  testnet4. Sharechain anchored to testnet4 block 154,600; genesis time set at launch.
- One Innosilicon S11 (about 4.2 TH/s) mining to that node over Stratum at difficulty 4096.
  Its power, fans and frequency were not changed; its normal pools were restored afterwards.
- No other Shoal node: this run tests the node against the public XBT network, not
  sharechain consensus between nodes (phase 2 tested that on regtest).

## Timeline (UTC)
- 2026-11-10 03:11: S11 on the Shoal node; first block 154,621 at 03:30.
- 2026-11-10 05:45: restarted on a fresh sharechain with the "genesis share is never paid"
  rule (Shoal pays no one for starting a chain; upstream pays the genesis share's key for
  14 days). The S11 fell back to its backup pool for 6 minutes (rejects only) and returned by
  itself. First block on the new chain: 154,628, its whole reward to the miner's address.
- 2026-11-12 13:24: the S11 moved to a second payout address, so that payouts split.
- 2026-11-13 ~06:10: the second address's shares first reach the paid part of the window
  (a block skips the newest 6,047 shares, upstream's coinbase-maturity rule).
- 2026-11-13 08:07: S11 back on its normal pools (the run was extended by 5 hours so the
  split could be seen).

## Results
| Question | Result |
|---|---|
| Do other XBT nodes accept Shoal's blocks? | Yes. Blocks 154,621-154,852: **200 Shoal, 32 other miners'**. Other miners built on ours from 154,643 (on our 154,642). A few of our blocks lost same-height races, as any miner's do |
| Is every coinbase the sharechain's PPLNS split? | Yes: **193/193** blocks on the final sharechain exact (81 that are shares, 112 that are not). The other 7 Shoal blocks belong to the first sharechain, replaced after 2.5 hours by the genesis-rule restart |
| Sharechain health | 26,735 shares at about 10 s each; 159 uncles (0.6%), **0 orphans, 0 invalid**, 0 copied shares, 0 shares on a made-up XBT block, 0 reorg alarms |
| Split payouts on testnet | Yes: blocks 154,845-154,850 pay both addresses, all six exact. The second address's cut grew as its shares entered the paid window (0.015 tXBT in the first, 0.27 in the second) |
| ASIC | The S11 ran at its usual 3.8-4.2 TH/s for 77 hours: about 57,000 Stratum shares accepted by the node, **0 rejected**. It restarted itself once (2026-11-10 ~10:08) without a gap in blocks |

## What the run taught us
- **Blocks that are not shares.** On testnet4 a block 20 minutes after the previous one may
  be mined at difficulty 1, far below the share difficulty (about 9,800). The S11 found most
  of its blocks that way: a submission met the block target but not the share target, so the
  node published the block but no share records it. Its coinbase still pays the PPLNS window.
  The checker now checks these too (it finds the parent share by trying the shares around the
  block). On mainnet a block is always far harder than a share, so this cannot happen there.
- **Maturity delays a miner's first payout.** A block pays the window from 6,048 shares back
  (about 17 hours at 10 s shares, upstream's rule, so that a sharechain reorg can't pay
  immature work). A new miner is paid by no block for its first 17 hours. Worth stating
  plainly to miners.
- **The genesis share.** Upstream pays the genesis share to a fixed key. Shoal pays no one;
  the first share of a chain pays the node's own address.

## Open items
- Shoal DNS seeds and a second, independent public node (peer-to-peer on testnet4 is
  still untested here).
- Offer the #652 work (mine on the current chain) upstream.
- A mainnet alpha needs the mainnet anchor chosen and the project owner's go.
