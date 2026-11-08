# Shoal

A decentralized pool for Bitcoin on BLAKE2b (XBT, Knots-BLAKE2b) with no operator. Each miner runs
their own Knots node and a Shoal node, which builds the block templates and serves their ASICs over
Stratum. A peer-to-peer sharechain keeps the accounting, and every block pays everyone in the
window straight from the coinbase. Nobody holds
the payee list, so there is no operator to cap, pressure or punish.

A shoal is a school of fish that moves together with no leader.

Shoal is written in Rust. Everything it ships is Rust: the sharechain core (a fork of P2Poolv2), the
XBT header and PoW, and the Stratum server. The Python in `model/` is analysis only, not shipped.

**Status: phase 2, regtest prototype. Regtest only for now (testnet4 next); nothing here is for
mainnet, and the node refuses testnet4 and mainnet until XBT anchors are chosen.**

| Milestone | Result |
|---|---|
| M0 fork spike | Done: P2Poolv2 forked, XBT header behind its API ([research/fork-spike.md](research/fork-spike.md)) |
| M1 one miner | Done: XBT ASIC-dialect Stratum, every block accepted by Knots, coinbase = sharechain PPLNS to the satoshi |
| M2 three nodes | Done: three nodes, 31 minutes, 19 blocks from three miners, identical sharechain tips, every payout exact |
| M3 kill a node | Done: the founding node hard-killed mid-run; the other two kept mining and paying it, and it resynced in 2 s on restart |
| M4 latency and uncles | Done: ~200 ms round trips between nodes, 10 s shares: 3.2% uncles, no orphans, no work lost, every payout exact including uncles |
| M5 attacks | In progress: a node mining on a dead XBT parent was still paid 18.5% of real coinbases; the fix (only pay shares built on the real XBT chain) is being built |
| M6 | Report ([plan.md](plan.md)) |

The fork's full test suite passes on XBT (1,589 tests).

| File | What |
|---|---|
| [design.md](design.md) | Sharechain format, how DATUM gateways plug in, payouts and coinbase limits, small miners, latency, attacks |
| [plan.md](plan.md) | Phase 2: a three-miner regtest prototype |
| [research/prior-art.md](research/prior-art.md) | Survey of P2Pool v1, P2Poolv2, Monero P2Pool, c2pool, Braidpool, Hashpool and others |
| [crates/shoal-header](crates/shoal-header) | XBT 164-byte header v2 and block types with a rust-bitcoin-shaped API |
| [crates/shoal-stratum](crates/shoal-stratum) | The Stratum V1 dialect XBT ASICs speak: job notify, submit parsing, header rebuild |
| [crates/shoal-grind](crates/shoal-grind) | Multi-threaded CPU test miner for regtest |
| [e2e/](e2e/) | Regtest end-to-end runs (`m1-single-miner.sh`, `m2-three-nodes.sh`) and the independent payout checker `verify_pplns.py` |
| [AwokenLazarus/p2poolv2](https://github.com/AwokenLazarus/p2poolv2) (`shoal/*` branches) | The sharechain core: our fork of P2Poolv2 |
| [model/](model/) | `sharechain_model.py`: income noise, payees per coinbase, stale rates (`python3 model/sharechain_model.py`) |

## Credits
- The sharechain core is a fork of [P2Poolv2](https://github.com/p2poolv2/p2poolv2) (MIT OR
  Apache-2.0); not affiliated with or endorsed by its developers. Rules also draw on Monero P2Pool.
- The design's DATUM front end (a later option) builds on FlyTheElephant's datum_gateway.

## Licence
Dual-licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
Contributions are accepted under the same terms.
