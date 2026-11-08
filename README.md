# Shoal

A decentralized pool for Bitcoin on BLAKE2b (XBT, Knots-BLAKE2b) with no operator. Miners build
their own block templates with their own node and DATUM gateway. A peer-to-peer sharechain keeps the
accounting, and every block pays everyone in the window straight from the coinbase. Nobody holds
the payee list, so there is no operator to cap, pressure or punish.

A shoal is a school of fish that moves together with no leader.

Shoal is written in Rust. Everything it ships is Rust: the sharechain core (a fork of P2Poolv2), the
XBT header and PoW, and the Stratum server. The Python in `model/` is analysis only, not shipped.

**Status: phase 1, design only. No code yet. Regtest and testnet4 only; nothing here is for mainnet.**

| File | What |
|---|---|
| [design.md](design.md) | Sharechain format, how DATUM gateways plug in, payouts and coinbase limits, small miners, latency, attacks |
| [plan.md](plan.md) | Phase 2: a three-miner regtest prototype |
| [research/prior-art.md](research/prior-art.md) | Survey of P2Pool v1, P2Poolv2, Monero P2Pool, c2pool, Braidpool, Hashpool and others |
| [model/](model/) | `sharechain_model.py`: income noise, payees per coinbase, stale rates (`python3 model/sharechain_model.py`) |

## Credits
- Sharechain rules follow Monero P2Pool's design; the planned sharechain core is a fork of
  P2Poolv2.
- XBT miners connect through DATUM gateways, including FlyTheElephant's datum_gateway.

## Licence
Dual-licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
Contributions are accepted under the same terms.
