# Shoal — design (phase 1, 2026-11-08)

Shoal is a sharechain pool for Bitcoin on BLAKE2b (XBT, Knots-BLAKE2b) with no operator. A shoal
is a school of fish that moves together with no leader.

Status: design only. Nothing is built yet. Numbers come from
`model/sharechain_model.py` (output in `model/sharechain_model.out`) unless a source is named.
Prior-art details and links: `research/prior-art.md`.

## 0. What we are building, in one paragraph

Every participating miner runs three things on their own hardware: a Knots-BLAKE2b node, a
DATUM gateway (unchanged, split-capable build), and a new daemon, `shoald`. To the
gateway, `shoald` looks exactly like a DATUM Prime on localhost. It hands out the coinbase
split, checks shares, and promotes the shares that meet the sharechain target to a
peer-to-peer **sharechain** that all `shoald` daemons keep. Every share's coinbase pays the
PPLNS split of its parent sharechain state, so any participant's block pays everyone in the
window, and nobody holds the payee list. ASICs keep speaking Stratum V1 to the gateway as they do
today. The miner's own node still builds every template; the sharechain never sees or chooses
transactions.

```
ASICs --SV1--> datum_gateway --DATUM (localhost)--> shoald <--p2p sharechain--> other shoald
                    |                                    |
                    +--GBT/submitblock--> own Knots-BLAKE2b node <--RPC (tip, submit)--+
```

## 1. Why XBT is a better home for a sharechain than Bitcoin was

1. **The ASIC never sees the coinbase.** A BLAKE2b miner is sent `coinb1 = 000000 ‖ h2 ‖ 00000000`
   (AwokenLazarus/Bitcoin, `lazarus/protocol/src/pow.rs`, `coinb1_sia`). `h2` commits to the merkle root, and the
   extranonce lives in the header, not the coinbase. So coinbase size never reaches firmware:
   the SHA-256 problem where Antminer stock firmware chokes on long `coinb2` (DATUM's coinbase size
   classes) does not exist here. This is what "we can have lots of outputs" means.
2. **Every serious miner already runs a local template node and an SV1 server**, the DATUM
   gateway. At Lazarus Pool, 206 miners do, carrying all but 0.3% of its hash (public pool API,
   2026-11-08). P2Pool v1
   on Bitcoin had to ship its own stratum server and template logic. Here the p2pool daemon only
   has to be a *local Prime*, a protocol already implemented in Rust (AwokenLazarus/Bitcoin `prime/wire`, MIT).
3. **Blocks are small and the chain is quiet:** blocks under 300 KB, RDTS weight cap 800k WU, so
   a few KB of payout outputs costs nothing that matters.
4. **The politics point the same way.** Knots' step-3 rule targets blocks built by
   "non-miners" (stratum-to-pool). A sharechain block is miner-built by construction, and there is
   no operator to cap, pressure or punish.

## 2. Prior art and fork base

Full survey with versions, licences and links: `research/prior-art.md` (2026-11-08).

| Project | Language, licence, state | Use it as |
|---|---|---|
| **P2Poolv2** (github.com/p2poolv2/p2poolv2) | Rust, MIT/Apache-2.0; v0.16.3 on 2026-11-06, ~330 commits in 12 weeks; testnet4 only | **Fork base for the sharechain core.** 10 s shares, ASERT difficulty, up to 3 uncles at depth 3, PPLNS in the coinbase, every node rebuilds and checks the payout, its own SV1 server. It has already fixed the copied-share bug class that hit Monero P2Pool in June 2026. |
| **Monero P2Pool** | C++, GPL-3.0; mainnet for years | **Design reference** for sharechain rules: 10 s shares, 2160-share window, uncles, mini/nano chains for small miners, and its June 2026 critical bug (GHSA-fm6j-gf38-p925: one share copied ~12,000 times) as a test case. |
| **c2pool**, binary `c2pool-bip110` | C++, AGPL-3.0; very active; labelled experimental | **The first BLAKE2b sharechain, already live on this chain.** A C++ reimplementation of P2Pool v1 (30 s shares, 8640-share window, no uncles). Reference for header v2 and Stratum handling, and a possible collaborator. Not a fork base (AGPL C++, v1 design). |
| **RATUM** | Rust, AGPL-3.0 | Reference for the Sia-dialect SV1 and coinbase sizing on header v2. Read, never copy. |
| **Braidpool** | Rust, AGPL; research | Design reference only (DAG of sub-second shares, FROST payout). It solves problems this chain does not have. |
| **Hydrapool** | Rust, AGPL | Not a base: centralized PPLNS/solo packaging of P2Poolv2's library; P2Poolv2 now has the same mode built in. |
| P2Pool v1 (Python 2), Hashpool (ecash shares), SV2 Job Declaration, GridPool | | History and design references. |
| `prime/wire`, `prime/tides` (AwokenLazarus/Bitcoin) | Rust, MIT | **Reuse:** DATUM Prime protocol, BLAKE2b v2 PoW, share rebuild, tested against the XBT DATUM gateways. |

### Recommendation
Fork P2Poolv2 for the sharechain core (consensus, uncles, p2p, storage: the parts that are hardest
to get right and where Monero's bug shows the cost of getting them wrong). Shoal adds, as its own
crates or a patch series on top of the fork:

1. **BLAKE2b header v2 and PoW.** P2Poolv2's `ShareHeader` embeds rust-bitcoin's 80-byte `Header`
   (about 30 call sites, plus PoW checks in `shares/validation` and `stratum/work/difficulty`).
   Either a local 164-byte header type or a patched rust-bitcoin (Braidpool's test chain did the
   latter). PoW from `prime/wire`.
2. **A DATUM-Prime front end** (`shoald` as a local Prime, from `prime/wire`). With it, existing
   gateways keep handling ASICs and the Sia-dialect Stratum, so P2Poolv2's own SV1 server, the
   biggest port job in the survey, is not needed in v1.
3. **XBT parameters:** GBT `blake2b` rule, window and maturity retuned for ~535 s blocks (P2Poolv2's
   120,960-share, 14-day window is sized for Bitcoin), RDTS output limits.

**Decision gate (phase 2, M0):** a two-day spike measures how deep the 80-byte header assumption
goes. If the header change reaches into P2Poolv2's consensus serialization in a way that cannot be
kept as a rebasable patch series, Shoal builds its own small sharechain on `prime/wire` instead,
following Monero's rules. Upstream churn is real (two P2Poolv2 chain resets in October 2026), so
the fork pins a tag and rebases deliberately.

**Also decide:** whether to work with c2pool-bip110 instead of beside it. It already runs on this
chain. Shoal differs in uncles and 10 s shares, in using the miners' own DATUM gateways and full
nodes for templates, and in being Rust and MIT/Apache. Two small sharechains would split the hash,
which is the one thing a sharechain can't afford.

Our own crates are used by path or git dependency, never copied. A third-party base is a real
GitHub fork with our changes on top, never vendored source.

## 3. Share format and share difficulty

### 3.1 What a share is
A share is a full XBT block header candidate (164 bytes, v2) that meets the **sharechain target**
and whose coinbase commits to a sharechain parent. On the wire:

| Field | Size | Notes |
|---|---|---|
| header | 164 B | v2 header as mined; `prev_block` must be a mainchain block at most 2 deep behind the receiver's tip (cf. `HEIGHT_LAG`) |
| coinbase tx | 2–9 KB | whole transaction in v1 (simple to verify); phase 3 can elide outputs that the receiver recomputes |
| coinbase merkle branch | ≤ 12 × 32 B | proves the coinbase is in `merkle_root` |
| sharechain fields | ~120 B | parent share id, up to 4 uncle ids, payout script (≤ 34 B, P2TR or P2WPKH), share `nBits`, share timestamp, version |

The coinbase carries a commitment output, `OP_RETURN PUSH36("XP2P" ‖ H(sharechain fields))`:
a 38-byte script. That fits under the RDTS 83-byte limit and, importantly, under the 64-byte limit
past which **stock DATUM gateways drop or truncate the whole coinbaser**
(AwokenLazarus/Bitcoin `research/node-template-consensus/gateway-readiness.md`). The reserved header slot
`m_mm_rhs` (bytes 132–163, consensus-inactive) could carry the commitment instead. We do not use
it, because it is the chain's merge-mining hook and Knots may give it a meaning.

### 3.2 Verification (every node, every share)
1. Header PoW: BLAKE2b two-pass + `xor_key` mask (`pow.rs` `pow_hash`) ≤ share target.
2. `prev_block` is a known mainchain block within the lag limit; share `nBits` equals the
   sharechain retarget for the parent.
3. Coinbase hashes into `merkle_root` via the branch.
4. Coinbase outputs, in order, equal `PPLNS(parent, V)` followed by the finder's remainder output
   and the witness commitment (the layout every DATUM gateway lineage produces:
   `datum_coinbaser.c`, outputs then "our payout" then witness commitment). `V` is the coinbase
   value the share claims; a share whose value exceeds subsidy + its claimed fees is rejected only
   if it is block-level (the node can't check fees for non-block shares, and it doesn't need to:
   over-claiming only inflates the share's own payout outputs, which are proportional to `V` for
   everyone).
5. The `XP2P` commitment matches the sharechain fields.

Cost: one BLAKE2b PoW, one coinbase hash, one branch, and a PPLNS lookup that is cached per parent.
Microseconds. At 10 s shares this is nothing.

Detail to settle in phase 2: how each gateway lineage (FlyTheElephant, CONVOY, Ratum,
lazarus-gateway) orders coinbaser outputs when it splits them across `coinb1`/`coinb2`, and
whether any of them reorders or drops a zero-value output. That fixes rule 4.

### 3.3 Share difficulty
- Target interval **10 s** (Monero P2Pool's value). XBT blocks arrive every ~535 s at today's
  hashrate, so a sharechain block contains ~540 shares at 5 PH/s and ~180 at 15 PH/s.
- Retarget every share from the last 2160 shares' work and timestamps, clamped (Monero-style), so
  share difficulty follows the sharechain's hashrate.
- A share's PPLNS weight is its **target difficulty**, not the hash it achieved (paying
  "whatever it reached" overpays and is gameable).
- Units: diff-1 = 2^32 hashes, as on the main chain. At 5 PH/s and 10 s the share difficulty is
  about 1.2e7 diff-1, about 1/540 of block difficulty (6.33e9).
- Gateway vardiff is untouched: the gateway still sends shares to the local daemon at its usual
  low difficulty. The daemon uses them for local stats and promotes only those that meet the
  sharechain target.

## 4. Sharechain consensus

- **Linear chain with uncles** (Monero design). A share may reference up to 4 uncles at depth ≤ 3.
  An uncle is paid 80% of its weight; the including share takes the other 20%. Fork choice is
  most cumulative work including uncles.
- Model, linear chain only: stale share rate at 10 s is 3% with 0.3 s relay and 9.5% with 1 s. With
  uncles almost all of those are paid at 80%, so the loss is a fraction of a percent.
- **PPLNS window: 2160 shares (~6 h at 10 s).** At 5 PH/s the pool finds a block every ~1.5 h, so
  about 4 blocks per window; at 1 PH/s, one every 7.6 h. PPLNS pays each share's expected value
  either way; a longer window only smooths income.
- Main-chain reorgs do not touch payouts. Payouts are fixed in each coinbase when it is mined; a
  reorg only changes which shares became blocks.
- Genesis, magic and ports differ per sharechain instance, so a "mini" chain is the same code with
  a different genesis (§6).

## 5. Payouts in the coinbase

### 5.1 Scheme
PPLNS over the window, by share weight, with integer pro-rata floors; the rounding remainder
and any unpayable weight (e.g. an output script that would be invalid) go to the finder's output.
It is TIDES-like (the window is in work, the coinbase pays the window) with no pool fee and no
pool output.

### 5.2 How many outputs fit (model §2)

| Sharechain hashrate | Window | Distinct payees (expected) | Coinbase bytes (P2TR) | With the NTA draft's per-payee attestation |
|---|---|---:|---:|---:|
| 1 PH/s | 2160 | 13 | 0.7 KB | 1.8 KB |
| 5 PH/s | 2160 | 58 | 2.6 KB | 7.2 KB |
| 15 PH/s | 2160 | 140 | 6.2 KB | 17.3 KB |
| 15 PH/s | 8640 | 180 | 7.9 KB | 22.1 KB |

Limits: a stock DATUM gateway's coinbase buffer is about 16.9 KB, which is 390 P2TR or 542
P2WPKH outputs (`datum_gateway.h:53-56`). Prime's coinbaser format caps 512 outputs. RDTS block
weight (800k WU) would take about 4,600 P2TR outputs. **At any size a sharechain is likely to reach,
outputs fit in stock gateways with a wide margin.** The one combination that does not fit is the
NTA draft (if it activated) at 15 PH/s or more; see §8.

Hard rule to keep it bounded: at most **380 payees** per coinbase. If a window ever has more,
the share pays the most recent shares that fit and drops the oldest from the window. That is
deterministic and only shortens the window.

### 5.3 Dust: there isn't any
The smallest thing a coinbase pays is **one share's weight**: about `V / 2160`, which is roughly
145,000 sats per block found at today's 3.125 XBT subsidy. That is far above dust (a TIDES pool needs a payout floor only because it pays by work, not by whole shares). No floor, no carry and no custodian are needed.
(Monero P2Pool works the same way.)

### 5.4 Small miners: income noise, not dust (model §1)
The question is how lumpy income is month to month: PPLNS pays every share for every block found
while it is in the window, so what counts is shares per month. An earlier internal triage (2026-09-25)
measured "chance of no share in one window", which overstated the problem.

| Miner | 1 PH/s sharechain | 5 PH/s | 15 PH/s | Solo (blocks / month) |
|---|---|---|---|---|
| 0.5 TH/s | 13% | 20% | 34% | 0.05 |
| 2.3 TH/s | 11% | 10% | 16% | 0.22 |
| 10 TH/s | 10% | 6% | 8% | 0.95 |
| 40 TH/s | 10% | 5% | 5% | 3.8 |

Values are month-to-month variation (coefficient of variation) of income, own shares and pool
luck combined. A 1-box miner on a 5 PH/s sharechain sees ±20%, against a 1-in-20 chance of
any block in a month mining solo. Above ~15 PH/s the main chain gets lumpy for the smallest
miners. That is when a **mini sharechain** (§6) is worth starting, not before.

### 5.5 Lightning payouts: not in v1
Every coinbase output is locked 180 days (Knots 29.4.3, #468, until RDTS expires 2027-09-01). Paying
anyone over Lightning before then means somebody fronts the money for 180 days, which is a
custodian or a lender, and that brings an operator back. Dropped for v1. Possible later as an
optional market (someone buys immature payouts for Lightning), outside the sharechain.

## 6. Small miners and the mini chain
Two instances of the same software:
- **main**: 10 s shares, for everyone.
- **mini**: 10 s shares on a separate sharechain with lower hashrate, so lower share difficulty.
  For miners under ~20 TH/s. In today's population those are 125 of 201 addresses holding 5% of the
  hash (0.78 PH/s per 14.7 PH/s). At 0.78 PH/s a 0.5 TH/s miner sees ±14% a month.

The mini chain splits the pool, which is its known drawback. The model says
it is not needed until main passes ~10–15 PH/s, so v1 ships main only, and mini is a config value.

## 7. SV1 miners, latency and orphans

- **SV1:** unchanged. ASICs keep pointing at their DATUM gateway. No firmware or ASIC change.
- **Without their own node:** a miner can point SV1 at someone else's gateway + `shoald`. That
  is a pool again (the host chooses templates and could withhold), so the docs discourage it and
  the sovereignty score (§8) shows it.
- **Share relay:** shares are 2–9 KB in v1 (full coinbase): 17–80 MB/day per peer link at 10 s. Fine
  for a home connection; outputs can be elided in phase 3.
- **Main-chain orphans:** a sharechain block is found and submitted by the finder's own gateway to
  its own node, exactly as DATUM does today. Other `shoald` nodes have only the coinbase and
  branch, not the transactions, so they cannot help relay the block. Orphan risk is the same as a
  DATUM miner's now, and depends on the finder's node peering. Phase 3: add a block-relay hint so
  that peers' nodes fetch it at once (compact-block `addnode` lists among participants).
- **Tip changes:** shares built on the previous mainchain tip (≤ 2 behind) stay valid sharechain
  shares and are paid, though they can't be blocks. Lazarus Pool does the same (`HEIGHT_LAG`); it
  recovered ~25% of work at short block intervals.

## 8. How LZT1, NTA and the maturity rules fit

- **LZT1 template attestation** (AwokenLazarus/Bitcoin `research/sovereign-tides`) proves that a named gateway
  key approved a template. On a sharechain every block is gateway-built by construction, and the
  canary detector has nothing to catch (there is no pool template to proxy). LZT1 is still worth
  carrying, optionally, for the public sovereignty score: it gives the sharechain's blocks
  "verified" status and lets anyone count independent builders. Caveat: the LZT1 output is 70 bytes,
  over the 64-byte limit of stock gateways, so it needs a patched gateway. That is optional in v1.
- **The NTA draft soft fork** (payees sign per height, tip and nBits; at most 512 payees) is the
  real interaction. Under NTA, a sharechain coinbase could only pay payees who had signed for
  *this tip*, so every payee in the window would have to be online and gossip a signature after
  every block. Offline payees would have to be carried. That is solvable (gossip signatures over the
  sharechain; payees without one for the current tip have their weight held in sharechain state),
  but it adds complexity and ~80 bytes per payee. **NTA is a draft by the same authors. Its text should
  allow sharechains**, for example with a longer-lived payee key authorisation
  instead of a per-tip signature.
- **180-day maturity** applies to every coinbase output on every pool, so the sharechain is
  neither better nor worse. It does mean new participants see no spendable income for six months.
- **Knots step 3 ("withhold rewards from stratum-to-pool blocks")**: a sharechain block is
  miner-built, so it should be on the right side of any rule. The risk is rule text that keys on
  "a coinbase paying many parties" or "a coinbase not paying the template builder". Watch it.

## 9. Attacks

| Attack | What happens | How bad | Mitigation |
|---|---|---|---|
| **Block withholding** (a participant, or a rival pool infiltrating with hash, submits shares but hides found blocks) | Earns PPLNS on others' blocks, never contributes its own | Real; the classic weakness of every pool with no operator. Unprofitable for a lone miner (it loses its own share of the hidden block). It can pay for a rival pool (miner's dilemma). | No operator to ban. Every share is public, so each address's rate of block-level shares versus share count is publicly auditable. But identities are free, so this deters only the careless. `xor_key` doesn't help, because here the template builder is the attacker. Bound: the loss is the withholder's fraction of the sharechain hash. Accept it, and publish the statistic. |
| **Invalid-template shares** (mine on templates that could never be blocks) | Same economics as withholding | Same | Same; plus block-level shares are checked by `submitblock` on the finder's node, and a share whose block is rejected is visible. |
| **Sharechain majority** (>50% of sharechain hash orphans others' shares) | Honest miners' later coinbases pay the attacker's window | **Serious while the sharechain is small.** About 20% of the network is rentable (MRR); 5 PH/s for a few hours is cheap. | Uncles; deep-reorg alarms (a node refuses a sharechain reorg that drops more than N of its own recent shares and alerts); grow past the rentable size. Monero has the same exposure and lives with it. |
| **Sybil** | Identities are free, but shares cost PoW | None for payouts | Relay only shares that meet the target; per-peer rate limits. |
| **Eclipse / p2p partition** | A partitioned node mines on a stale sharechain | Its own loss | Outbound peers from several seeds; `addnode` lists; alarms when the sharechain tip ages. |
| **Difficulty / timestamp games on the sharechain** | Slight share-difficulty skew | Small | Clamped retarget, median-time rules (Monero-style). |
| **Pool hopping** | | None | PPLNS resists it. |
| **Gateway bugs** (empty / pool-only coinbase jobs, DATUM self-race) | Shares on such jobs don't commit to the sharechain | That miner's loss only | Require split-capable gateways; the daemon refuses to issue "empty" coinbaser jobs and logs the gateway lineage. |
| **Main-chain selfish mining by a big participant** | As on any pool | Same as today | Not made worse; the sharechain is public, so withheld main-chain blocks are no easier to hide. |

## 10. Why P2Pool v1 faded, and whether that applies here

| v1 problem | Here |
|---|---|
| Share difficulty grew with the pool; small miners got nothing | Model §1: fine to ~15 PH/s; mini chain after |
| Stale shares at 30 s / 10-min blocks; no uncles | Uncles (Monero design) |
| Huge coinbase choked ASIC firmware and blocks | ASIC never sees the coinbase (§1.1) |
| Miners had to run bitcoind + p2pool + stratum | Miners here already run node + gateway; we add one daemon |
| Python 2, unmaintained | Rust, our own protocol crates |
| No income to pay for maintenance | Still true. Funded as public good / research, by design |
| Users preferred convenience | **Still the biggest risk.** Monero's P2Pool reached a large share of Monero hash only after years and a privacy motive. Here the motive is the cap fight. |
