# Shoal — phase 2 plan (regtest prototype)

Not started. Phase 2 begins once the phase 1 verdict is accepted.

## Goal of phase 2
Show, on regtest only, that three miners with **no pool operator** can mine into one sharechain,
and that every block any of them finds pays all three by PPLNS straight from the coinbase, with
templates built by each miner's own node through an unmodified (split-capable) DATUM gateway.

## Where it runs
- A lab VM, not a shared coordination host: 3 × (Knots-BLAKE2b regtest node + datum_gateway +
  `shoald`) plus a miner. About 4 cores and 8 GB is enough. Builds and tests run there.
- Regtest only, with RDTS on (`-rdtsexpiry=4102444800`) and the 180-day maturity rule set
  (`-testcoinbasematuritylong=...`), so the tests see what mainnet enforces. Without
  `-rdtsexpiry`, RDTS is off on regtest and tests pass things mainnet refuses.
- Test miners always run under `timeout` in their own process group and are reaped on exit.

## Code layout
- This repository: design, model, and Shoal's own crates.
- Sharechain core: a fork of P2Poolv2 (see design.md §2), pinned to a tag, with Shoal's changes as
  a rebasable patch series. Never vendored source.
- `prime/wire` and `prime/tides` from AwokenLazarus/Bitcoin (MIT) as git dependencies: DATUM Prime
  protocol, BLAKE2b v2 PoW, share rebuild. Never copied.
- Docs that use FlyTheElephant's datum_gateway credit it.

## Milestones

| # | What | Proves | Done when |
|---|---|---|---|
| M0a | Gateway layout: build FlyTheElephant, CONVOY and lazarus-gateway against regtest; dump the exact coinbase each makes from a 60-output coinbaser plus a 38-byte zero-value `XP2P` OP_RETURN | Gateways keep the commitment and order outputs predictably (design §3.2 rule 4) | A table per lineage in `research/gateway-layout.md` |
| M0b | Fork-base spike (2 days): swap P2Poolv2's 80-byte header for header v2 behind a type, run its share-validation tests | The fork is viable as a patch series | Go: fork. No-go: own sharechain on `prime/wire` with Monero's rules |
| M1 | `shoald` local Prime: accepts one gateway, serves PPLNS coinbasers from the sharechain, verifies shares, promotes those over the share target | One miner, shares form a chain, blocks pay the window | A regtest block's coinbase equals `PPLNS(parent)` to the sat |
| M2 | Three nodes: gossip, parent/uncle links, fork choice by work, retarget, short PPLNS window on regtest (e.g. 60 shares) | Nodes agree | 30 min run: identical sharechain tips; every block found by any node pays all 3 by share weight, checked by an independent script that reads only the chain and the published shares |
| M3 | No operator | Any node can die | Kill any one node mid-run: the other two keep mining and paying; the restarted node resyncs from peers |
| M4 | Latency and uncles | Stale shares are paid | `netem` 100–300 ms between nodes: uncle rate measured, weight lost < 1% |
| M5 | Attacks, small | Rules hold | (a) a node serving a wrong split has its shares rejected; (b) a withholding node's shares are accepted, its blocks never appear, and the public statistic shows it; (c) a private sharechain fork with more work trips the reorg alarm; (d) Monero's GHSA-fm6j-gf38-p925 case: a copied share is rejected |
| M6 | Report | | `phase2-result.md`; verdict for phase 3 (testnet4, then a public alpha) |

## Not in phase 2
Mini chain (config only), output elision, LZT1 in shares, NTA compatibility, Lightning,
testnet4, mainnet, any announcement.

## Risks to watch
- A gateway lineage drops or reorders zero-value outputs (M0a decides which gateways are supported).
- `prime/wire` assumes one Prime per pool; a local Prime must serve a fresh PPLNS coinbaser every
  time the sharechain tip moves (every ~10 s), not per template. Measure coinbaser latency.
- Regtest difficulty is tiny: use a fixed share target or a faster miner so the 10 s cadence is real.

## Estimate
M0–M3: about 2–3 weeks of focused work. M4–M6: about 1 more.
