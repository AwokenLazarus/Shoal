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

# Fork spike, part 2: results on xbt-lab (2026-11-08)

Lab: xbt-lab (8 vCPU, capped), Rust 1.99.0 (P2Poolv2's pinned toolchain), P2Poolv2 tag v0.16.3
(`a6f39146`). Command: `cargo test -p p2poolv2_lib --no-fail-fast`.

| Run | Passed | Failed |
|---|---:|---:|
| Stock v0.16.3 (baseline) | 1,315 | 0 |
| With `shoal-header::XbtHeader` / `XbtBlock` in place of rust-bitcoin's `Header` / `Block` | 1,271 | 44 |

**The change:** `shoal-header` (this repo; 13 tests, clippy and fmt clean) plus a patch to
P2Poolv2 of 14 files (12 source files plus Cargo.toml and Cargo.lock), 34 lines added and 18 removed. It touches only imports, header struct literals
(which gain a `v2` field), the share header's encoder/decoder (which now carries the 84 v2 bytes),
and `submit_block`, made generic over the block type. No consensus rule was rewritten. The local
branch is `shoal/xbt-header`, not on GitHub.

**All 44 failures are test data, not logic:**

| Count | Cause | Fix (M1) |
|---:|---|---|
| 19 | JSON fixtures saved with the 80-byte header (no `v2`), mined with SHA-256d nonces | Regenerate fixtures on XBT regtest |
| 19 | Sharechain genesis anchored to a Bitcoin block (`*.dat`); `XbtHeader` correctly refuses its v1 header | XBT genesis: a regtest block after the BLAKE2b fork height, later testnet4 |
| 6 | Stratum tests replaying real SHA-256 ASIC submissions, now (correctly) below difficulty | Replace with Sia-dialect BLAKE2b submissions from our CPU miner, then from a real ASIC |

No share-validation, PPLNS, uncle, ASERT or p2p test failed for any other reason.

**Verdict: GO.** P2Poolv2 takes the XBT header as a small, rebasable patch series. That meets
"fork, don't vendor". Next: the GitHub fork (needs the owner's yes), then M1 (v2 fields from the
real Stratum job, XBT genesis, regenerated fixtures, Sia-dialect Stratum).
