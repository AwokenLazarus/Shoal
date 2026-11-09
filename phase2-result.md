# Shoal — phase 2 result (regtest prototype)

2026-11-09. Phase 2 asked one question: on regtest, can three miners with **no pool operator**
mine into one sharechain, with every block any of them finds paying all of them by PPLNS straight
from the coinbase, and with every template built by each miner's own node? Answer: **yes**. The
rules also held against the attacks we tried.

## Verdict
**Go for phase 3** (XBT testnet4, then a public alpha), with the open items below.

## What was built
- **The fork** ([AwokenLazarus/p2poolv2](https://github.com/AwokenLazarus/p2poolv2), branch
  `shoal/xbt-header`): P2Poolv2 v0.16.3 changed for XBT. It has the 164-byte header v2 and
  BLAKE2b proof of work, the Stratum dialect XBT ASICs speak, regtest sharechain parameters, and
  **"mine on the current XBT chain"**. That last one is upstream's own design for its issue #652,
  not yet built upstream; we built it for XBT. The full test suite passes (see below).
  `SHOAL-CHANGES.md` lists every change.
- **This repo:** the XBT header and Stratum crates, a CPU test miner, the end-to-end scripts in
  `e2e/`, and `e2e/verify_pplns.py`. The checker recomputes every coinbase from the sharechain
  and the XBT chain alone, and never asks a Shoal node what it should be.

## Results

Every run: three Knots-BLAKE2b regtest nodes, three Shoal nodes, three CPU miners on one lab VM.
"Exact" means `verify_pplns.py` rebuilt every checked coinbase to the satoshi.

| Milestone | Run | Result |
|---|---|---|
| M1 one miner | `e2e/m1-single-miner.sh` | Every block accepted by Knots; every coinbase = the sharechain's PPLNS split |
| M2 three nodes | 31 min | 19 blocks found by all three miners; identical sharechain tips; 19/19 exact |
| M3 kill a node | founding node SIGKILLed at 10 min, down 5 min | The other two kept mining and paying it; it resynced in 2 s; 20/20 exact |
| M4 latency | 100 ± 50 ms each way between nodes (~200 ms round trip), 10 s shares, 30 min | 3.2% uncles, 0 orphans, 0% work lost; 168/168 exact (61 paying uncles). Stress run at 1.3 s shares: 15.5% uncles, 2.4% orphans, 1,171/1,171 exact |
| M5 honest, with the payout rule | 15 min | 75/75 exact; identical tips; no honest share ever judged ineligible |
| M5 work on a fake XBT block | node 3 builds every job on a made-up XBT parent, 20 min | Before the fix: those shares took 18.5% of real coinbases. After: honest nodes dropped all of them (~70 each), the attacker got **0%**; 103/103 exact |
| M5 wrong payout split | node 3's coinbases pay everything to itself, 15 min | Its shares are invalid on honest nodes; it got **0 sats**; 51/51 exact |
| M5 copied shares (Monero P2Pool GHSA-fm6j-gf38-p925) | node 3 rebroadcasts every share with a changed coinbase and under its own address, 15 min | Every copy rejected; **0** shares share a proof of work; the copier got 0; 85/85 exact |
| M5 block withholding | node 3 never submits blocks, 15 min | Its shares are accepted and paid, as designed (no pool can stop this), and the public statistic shows it: **29 shares that met the block target, 0 blocks**, against ~90% for honest miners |
| M5 private sharechain fork | node 3, with more hashrate than the other two together, mines alone for 5 min, then publishes | Its heavier chain replaced the honest one, and **both honest nodes raised the reorg alarm**; 9 honest shares lost; 73/73 exact |
| Mixed mempool policy | node 3's Knots refuses transactions under 50 sat/vB; a wallet sends one every 5 s at 1 or 100 sat/vB, 15 min (`MIXED_POLICY=1`) | Templates differed (node 3: 1 transaction in 24 blocks; nodes 1 and 2: 23 in 32 and 6 in 28); every block still paid the shared window: 84/84 exact, identical tips |
| Real ASIC (Innosilicon S11, 4.2 TH/s) | node 3 serves the S11 over the LAN at difficulty 4096, nodes 1 and 2 keep CPU miners, 40 min (`ASIC_NODE=3`) | The S11 ran at its usual hash rate (0 rejected and 0 hardware errors when sampled early in the run). 391 of its shares on the main chain, 377 of them XBT blocks; identical tips; 0 invalid; 460/460 exact |

Fork test suite (`cargo test --workspace`, xbt-lab): 1,613 passed, 0 failed, 6 ignored (fixture tools and DNS-seed tests).

## The fix that phase 2 needed: mine on the current XBT chain
The first attack run found a real hole, one upstream shares. Nothing tied a share's XBT block
to the real XBT chain. So a node could mine every job on a made-up XBT parent block, which can
never become a block, and still be paid. Before the fix, such a node took **18.5% of every real
coinbase** in a 20-minute run.

We built upstream's design for it:
- every node keeps a store of XBT headers. Its own Knots node feeds it, and so do headers that
  travel with shares, which each node checks itself: they must link up, carry real proof of
  work and the exact difficulty XBT's rules require, and not be dated in the future;
- a share is accepted only once its XBT parent is known;
- a coinbase pays a share only if that share's XBT parent is on the paying block's XBT chain, or
  one block off it, so a block that lost an orphan race is still paid. Other shares keep their
  place in the window and are paid nothing.

After the fix, the same attack gets **0%**. Honest nodes drop every share on the fake parent and
never store it. In honest runs no share was ever judged ineligible.

## Open items for phase 3
- **XBT testnet4 parameters**: the sharechain's genesis time and target are chosen at launch, so
  a months-old genesis can't pin the difficulty. Also needed: Shoal's DNS seeds and a long-lived
  host.
- **Coordinated validity changes**: the payout rule changes which coinbases are valid, so every
  node must run it from the start of a chain (true for any Shoal network so far).
- **Upstream**: offer the #652 work back to P2Poolv2 if they want it.
- **Not tested on regtest**: Knots-level reorgs of the XBT chain (the rule is built for them:
  orphans are paid, hostile forks get at most two blocks), and long runs.
- **Withholding** cannot be stopped by any pool. Shoal shows it publicly instead (see M5); the
  statistic lives in the checker for now and belongs in the node's API.
- **A majority of sharechain hashrate wins the sharechain**, as in any proof-of-work chain. The
  private-fork run shows the alarm fires and payouts stay correct; it does not and cannot stop it.
- **A start-up race seen and fixed**: a fresh node could get a peer's share before it had
  loaded any XBT headers, so it dropped the share once (it recovered by itself). The node now
  loads its Knots chain into the header store before it talks to peers.

## How to reproduce
On a lab machine with Knots-BLAKE2b built: `e2e/m2-three-nodes.sh`, with the environment shown
in each row above (`SECS`, `STRATUM_DIFF`, `ATTACK_NODE`, `ATTACK_MODE`, `NETEM_MS`,
`KILL_NODE`, `MIXED_POLICY`). Attack runs use a fork build with `--features shoal-attack` (or
`shoal-attack-dead-template`); those features refuse to run off regtest.
