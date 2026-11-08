# Shoal — phase 2 plan (regtest prototype)

Started 2026-11-08. **M0 done** (GO, research/fork-spike.md). **M1 done** (2026-11-08): one CPU
miner speaking XBT's ASIC dialect mines through Shoal on regtest; every block is accepted by Knots
and its coinbase pays the sharechain's PPLNS split to the satoshi (independent checker
`e2e/verify_pplns.py`; runs of 6 and 5 blocks, all OK). The fork's whole workspace test suite passes
on XBT (1,588 tests). Fork: github.com/AwokenLazarus/p2poolv2, branch `shoal/xbt-header`.
**M2 done** (2026-11-08, `e2e/m2-three-nodes.sh`): three Shoal nodes, each with its own Knots
node and CPU miner. Node 1 started the sharechain and nodes 2 and 3 joined over p2p, synced, then
served work. In a 31-minute run, 19 blocks were found by all three miners, none rejected, and the
three nodes ended on the same sharechain tip. Every coinbase matched the checker to the satoshi,
with regtest's 12-share window sliding and its maturity skip applied; the last blocks paid all
three miners. No uncles came up (shares about 85 s apart); uncles are tested in M4.
**M3 done** (2026-11-08, `KILL_NODE=1 e2e/m2-three-nodes.sh`): every node dials every other.
At 10 minutes node 1, the node that started the sharechain, was killed with SIGKILL along with its
miner. The other two kept mining; a block found while it was down paid the window, node 1's
earlier shares included. After 5 minutes node 1 restarted on its old store, resynced from peers in
2 s, then served work again. 35-minute run: 20 blocks, 0 rejected, identical tips, PPLNS 20/20 to
the satoshi.
Next: M4 (latency and uncles).

## Goal of phase 2
Show, on regtest only, that three miners with **no pool operator** can mine into one sharechain,
and that every block any of them finds pays all three by PPLNS straight from the coinbase, with
templates built by each miner's own node (GBT into `shoald`, which serves Stratum V1 to the miners;
see research/fork-spike.md).

## Where it runs
- A lab VM, not a shared coordination host: 3 × (Knots-BLAKE2b regtest node + `shoald`) plus a miner. About 4 cores and 8 GB is enough. Builds and tests run there.
- Regtest only, with RDTS on (`-rdtsexpiry=4102444800`) and the 180-day maturity rule set
  (`-testcoinbasematuritylong=...`), so the tests see what mainnet enforces. Without
  `-rdtsexpiry`, RDTS is off on regtest and tests pass things mainnet refuses.
- Test miners always run under `timeout` in their own process group and are reaped on exit.

## Code layout
- This repository: design, model, and Shoal's own crates.
- Sharechain core: github.com/AwokenLazarus/p2poolv2, a GitHub fork of P2Poolv2 based on tag v0.16.3,
  with Shoal's changes on `shoal/*` branches as a rebasable patch series. Never vendored source.
  Licence compliance: upstream LICENSE/AUTHORS/SPDX headers untouched, a modification notice in every
  changed file (Apache-2.0 §4(b)), `SHOAL-CHANGES.md`, and a not-affiliated notice.
- `lazarus-protocol` from AwokenLazarus/Bitcoin (MIT) as a git dependency: BLAKE2b header v2, PoW,
  Sia-dialect Stratum job building. Never copied.
- Docs that use FlyTheElephant's datum_gateway credit it.

## Milestones

| # | What | Proves | Done when |
|---|---|---|---|
| M0 | Fork-base spike part 2 (2 days; part 1 done in research/fork-spike.md): baseline `cargo test -p p2poolv2_lib` at v0.16.3, then swap in an `XbtHeader` type behind the existing API and rerun | The fork is viable as a patch series | Go: fork. No-go: own sharechain on `lazarus-protocol` with Monero's rules |
| M1 | `shoald` on XBT: GBT with the `blake2b` rule, Sia-dialect Stratum V1 (from our `lazarus-protocol`), BLAKE2b share PoW; one CPU/ASIC-emulating miner | One miner, shares form a chain, blocks pay the window | A regtest block's coinbase equals `PPLNS(parent)` to the sat |
| M2 | Three nodes: gossip, parent/uncle links, fork choice by work, retarget, short PPLNS window on regtest (e.g. 60 shares) | Nodes agree | 30 min run: identical sharechain tips; every block found by any node pays all 3 by share weight, checked by an independent script that reads only the chain and the published shares |
| M3 | No operator | Any node can die | Kill any one node mid-run: the other two keep mining and paying; the restarted node resyncs from peers |
| M4 | Latency and uncles | Stale shares are paid | `netem` 100–300 ms between nodes: uncle rate measured, weight lost < 1% |
| M5 | Attacks, small | Rules hold | (a) a node serving a wrong split has its shares rejected; (b) a withholding node's shares are accepted, its blocks never appear, and the public statistic shows it; (c) a private sharechain fork with more work trips the reorg alarm; (d) Monero's GHSA-fm6j-gf38-p925 case: a copied share is rejected |
| M6 | Report | | `phase2-result.md`; verdict for phase 3 (testnet4, then a public alpha) |

## Not in phase 2
Mini chain (config only), output elision, LZT1 in shares, NTA compatibility, Lightning,
testnet4, mainnet, any announcement.

## Carried into M3/M4
- Regtest sharechain parameters (fork `xbt_params.rs`): PPLNS window 12 shares, maturity 4,
  and a 64-entry cache buffer. `verify_pplns.py` mirrors the window, the maturity skip and
  uncle weighting (9/10 to the uncle's miner, 1/10 to the including share's miner).
- Stratum difficulty is an integer with a floor of 1 (about 2^32 hashes), so a CPU miner finds a
  share about every 4 minutes at 2 threads. M4's latency and uncle tests want more shares: add a
  regtest-only fractional difficulty.
- **M5 attack to test first: work on a dead template.** Share validation checks the payout,
  the coinbase-to-header link and the share's PoW, but nothing ties the share's XBT header to
  the real chain (prev block = a recent XBT block, bits = that height's network target). A miner
  could earn shares on work that can never be a block (a withholding equivalent). A fix must stay
  deterministic across nodes, so not "is it my node's tip right now". Options: a header-chain
  commitment checked against Knots with a depth allowance, or v1-style penalties in fork choice.
- Not yet tested: nodes with different mempools and policies (templates are local; only the
  payout is shared). Add to M4/M5: one node with a stricter policy, its blocks still pay all.
- The start-up "Method not found" is `getdifficulty`, which Knots-BLAKE2b doesn't have. Upstream
  calls it once, only to log the network difficulty. Harmless.

## Risks to watch
- Real BLAKE2b ASIC firmware vs our Stratum: regtest only has CPU miners; test against one real
  ASIC on regtest before any testnet (a Goldshell or Iceriver on the LAN, if one is available).
- P2Poolv2 upstream churn: pin v0.16.3, rebase deliberately, review its share validation ourselves.
- Regtest difficulty is tiny: use a fixed share target or a faster miner so the 10 s cadence is real.

## Estimate
M0–M3: about 2–3 weeks of focused work. M4–M6: about 1 more.
