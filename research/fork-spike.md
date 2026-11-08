# Fork spike, part 1: reading P2Poolv2 (2026-11-08)

Local clone of github.com/p2poolv2/p2poolv2 at tag **v0.16.3** (`3c4d3559`), read only. No GitHub
fork. Nothing built yet: builds and tests wait for the lab VM (plan M0b, part 2).

## What P2Poolv2's consensus is tied to

| Coupling | Where | Size of the change for XBT |
|---|---|---|
| **80-byte rust-bitcoin `Header`** inside every `ShareHeader` (`bitcoin_header`) | `shares/share_block/mod.rs`; 84 references in 12 files of `p2poolv2_lib`; 23 PoW / target / hash call sites | Medium. A `XbtHeader` newtype (164 bytes) with the same methods (`block_hash`, `target`, `validate_pow`, encode/decode), backed by our `lazarus-protocol` `pow.rs`. Mostly mechanical. |
| **SHA-256d PoW** | the same call sites, plus `stratum/work/difficulty` | Comes with the header type: BLAKE2b two-pass plus the `xor_key` mask. |
| **Txid and merkle tree** | `coinbase_proof.rs` (SHA-256 midstate of the coinbase prefix) | **None.** XBT still uses SHA-256d for txids and the merkle root (Knots `primitives/transaction.cpp`, `consensus/merkle.cpp`), so the midstate proof works unchanged. |
| **Coinbase layout: the share commitment is the last output before locktime, and the prefix is padded to 64-byte SHA-256 blocks** | `coinbase_proof.rs`, `stratum/work/coinbase.rs` | None if P2Poolv2 builds the coinbase itself. **Incompatible with DATUM gateways**, which put their own payout and the witness commitment last and don't pad. |
| **Stratum V1, Bitcoin dialect** | `stratum/work/{notify,prepared_notify,block_template,gbt}.rs` | Medium. Replace with the Sia dialect XBT ASICs speak (`coinb1 = 000000 ‖ h2 ‖ 00000000`, the extranonce in the header). Our own `lazarus-gateway` and `lazarus-protocol` (MIT) already implement it. |
| **GBT** | `stratum/work/gbt.rs`, `bitcoindrpc` | Small: add the `blake2b` rule and read the v2 fields (`xor_key`, height, reserved slot). |
| Optional fee and donation outputs (`fee_address`, `donation_address`) | `ShareHeader`, `accounting/payout` | Config only. Shoal ships with both off. |
| "Top-N miners in the coinbase, others via market makers / atomic swaps" | README, `docs/atomic-swap` (not shipped) | Not needed. On XBT every payee fits (design.md §5.2), and one share is already well above dust. |

## What this changes in the design

The design (design.md §0) put Shoal **behind the miners' DATUM gateways** as a local Prime. That
fights P2Poolv2's coinbase rule: the gateway, not the sharechain, decides the coinbase layout. Making
it fit would mean either changing P2Poolv2's share consensus (the commitment position and the proof)
or patching every gateway lineage.

**Recommended instead (Path 1):** use P2Poolv2's own design. Each miner runs **their own Knots node
plus `shoald`**. `shoald` gets templates from that node by GBT and serves Sia-dialect Stratum V1 to
the ASICs directly.
- Template sovereignty is the same: the miner's own node picks every transaction.
- **Two programs instead of three**: simpler to install, and fewer gateway-compatibility bugs. Those
  bugs have cost XBT pools real blocks: empty-first jobs, discarded coinbasers, outdated nodes.
- P2Poolv2's share consensus (commitment, midstate proof, uncles, PPLNS, header batches) stays as
  upstream wrote and tested it. Our changes are the header/PoW type, the Stratum dialect, GBT, and
  parameters. That is a rebasable patch series, which is what "fork, don't vendor" needs.
- Miners who use a DATUM gateway today point their ASICs at `shoald`'s stratum port instead.
  The gateway isn't needed for Shoal.

**Path 2 (kept as a later option):** a DATUM-Prime front end. It needs a gateway patch that appends
the commitment after the witness commitment (BIP141 allows that: the commitment is the
highest-index matching output) and pads the prefix. Only worth it if miners demand to keep their
gateways.

## Safety notes from the read
- P2Poolv2 already guards against several share-copy / malleability classes: an address is stored as
  a witness program so one output can't be spelled four ways; the merkle branch is capped at 16; the
  miner is bound to the PoW through the commitment. Shoal's own tests still have to cover Monero's
  GHSA-fm6j-gf38-p925 case (one share copied many times).
- It is testnet4-only, and had two chain resets in October 2026. Shoal pins v0.16.3 and does its own
  review of the share-validation path before any public testnet.

## Part 2 (on the lab VM)
1. `cargo test -p p2poolv2_lib` on stock v0.16.3: the baseline.
2. Introduce `XbtHeader` behind the existing API; rerun. Count every test that had to change.
3. Go / no-go: if the header swap stays outside consensus serialization other than the header
   bytes themselves, fork. Otherwise our own sharechain on `lazarus-protocol`.
