# Prior art: decentralized share-chain pools (survey, 2026-11-08)

Scope: choose a **fork base** and **design references** for a Rust, Stratum-V1, coinbase-payout
share chain on the Knots BLAKE2b chain (XBT, BIP-110 hard fork, 164-byte header v2).

Method: GitHub metadata via `gh api repos/<o>/<r>` and `.../commits?per_page=3` (read-only, 2026-11-08);
shallow clones read in a scratch dir (nothing built, nothing forked, starred or posted); web pages where noted.
"12-wk commits" is the sum of the last 12 weeks of `gh api repos/<o>/<r>/stats/participation`.
**[unverified]** marks anything I could not confirm from a primary source.

---

## 0. Summary table

| Project | Repo | Lang | License | Last commit / release | 12-wk commits | Role for us |
|---|---|---|---|---|---|---|
| P2Pool (orig.) | https://github.com/p2pool/p2pool (forrestv/p2pool redirects here) | Python 2 | GPL-3.0 | 2018-09-19 / none | 0 | design reference (dead) |
| jtoomim p2pool | https://github.com/jtoomim/p2pool | Python 2 / PyPy | GPL-3.0 | 2022-06-16 / 33.3 (2017-09-18) | 0 | design reference (dead) |
| **P2Poolv2** | https://github.com/p2poolv2/p2poolv2 (pool2win/p2pool-v2 redirects here) | Rust | **MIT OR Apache-2.0** | **2026-11-07 / v0.16.3 (2026-11-06)** | **329** | **fork base** |
| Hydrapool | https://github.com/256foundation/hydrapool | Rust | AGPL-3.0 | 2026-06-12 / v2.6.0 (2026-06-12) | 0 | packaging of P2Poolv2 lib (centralized mode) |
| Monero P2Pool | https://github.com/SChernykh/p2pool | C++ | GPL-3.0 | 2026-10-24 / v5.0-beta (2026-10-01), v4.18.1 (2026-09-22) | 9 | design reference (best production record) |
| Braidpool | https://github.com/braidpool/braidpool (`dev` branch active; `main` last 2025-06-02) | Rust node + TS dashboard | AGPL-3.0 | dev 2026-11-06 / no releases | 0 on main | design reference (research) |
| **c2pool** | https://github.com/frstrtr/c2pool | C++ | AGPL-3.0 | 2026-11-08 / v0.2.9 (2026-11-07) | 1308 | **direct competitor / reference: already has a BIP-110 BLAKE2b sharechain binary** |
| DATUM Gateway | https://github.com/OCEAN-xyz/datum_gateway | C | MIT | 2026-10-19 / v0.5.1beta | 56 | template source / SV1 front end |
| RATUM | https://github.com/iohzrd/ratum | Rust | AGPL-3.0 | 2026-10-15 / – | – | **reference for v2-header Stratum (Sia dialect) in Rust** |
| SV2 apps (SRI) | https://github.com/stratum-mining/sv2-apps (+ https://github.com/stratum-mining/stratum, v1.13.0 2026-11-04) | Rust | MIT/Apache (NOASSERTION on GitHub) | 2026-11-06 / v0.9.2 | 310 | reference (Job Declaration) |
| Hashpool | upstream https://github.com/vnprc/hashpool **now 404**; latest fork seen https://github.com/wilfredallyn/hashpool | Rust | NOASSERTION | upstream commits to 2025-12-12 (via fork) / v0.1.1 [per search snippet] | – | design reference (ecash shares) |
| GridPool (BLAKE2b) | https://github.com/gridlabs-science/gridpool-blake2b | C# | MIT | 2026-09-04 | – | reference (no-sharechain coinbase-list sharing) |
| Public Pool | https://github.com/benjamin-wilson/public-pool | TypeScript | GPL-3.0 | 2026-06-05 | 0 | not relevant (solo) |
| ckpool | https://github.com/ckolivas/ckpool | C | GPL-3.0 | 2026-10-30 | 46 | not relevant (centralized/solo) |

---

## 1. Original P2Pool and jtoomim's fork

- Repo: https://github.com/p2pool/p2pool. Last code commit 2018-09-19 ("Update bitcoin.py"), no releases, 0 commits in 12 weeks (GitHub `pushed_at` 2026-02-28 is not a code commit). Python 2 + Twisted. GPL-3.0.
- jtoomim fork: https://github.com/jtoomim/p2pool. Last commit 2022-06-16, last release 33.3 (2017-09-18). Python 2 with PyPy recommended; aimed at BTC/BCH with big-block performance fixes.
- Bitcoin network params (both forks, `p2pool/networks/bitcoin.py`): `SHARE_PERIOD = 30` s, `CHAIN_LENGTH = REAL_CHAIN_LENGTH = 24*60*60//10 = 8640` shares, `SPREAD = 3` blocks. Payout: PPLNS paid in the coinbase, one output per miner. No uncles. Orphaned/dead shares are lost, which hurts high-latency and small miners.
- Why it died on Bitcoin (sources: Braidpool roadmap https://github.com/braidpool/braidpool/blob/dev/docs/roadmap.md): very large coinbases (Bitmain S5 firmware could not take them), 30 s shares too slow for high hashrate, and Python performance.
- **Modularity:** good. PoW is one line per network, `POW_FUNC = data.hash256` in `p2pool/bitcoin/networks/bitcoin.py`. That is why dozens of altcoin forks exist. The 80-byte header is defined in `p2pool/bitcoin/data.py` (`block_header_type`). Stratum V1 is built in (`p2pool/bitcoin/stratum.py`). Templates come only from the local node through getblocktemplate (GBT). There are no external template providers.
- **Verdict:** design reference only. Python 2, unmaintained, GPL.

## 2. P2Poolv2 (pool2win) and Hydrapool (256 Foundation)

Repo: https://github.com/p2poolv2/p2poolv2 (the old pool2win/p2pool-v2 URL redirects here). Rust (MSRV 1.88, edition 2024). Dual MIT/Apache-2.0. Very active: 329 commits in 12 weeks; v0.16.3 on 2026-11-06, v0.16.0 on 2026-10-19 (consensus and store reset). Crates: `p2poolv2_lib`, `_node`, `_api`, `_cli`, `_config`, `_wallet`, `_electrum`, `_sim`, `bitcoindrpc`. About 82k lines of Rust in `p2poolv2_lib/src`. There is a TLA+ spec in `spec/` and a nightly multi-node simulation. Networking is libp2p (Kademlia, request-response). Storage is RocksDB.

**Design (from README and `docs/architecture/*`):**
- Mission and scheme (README): "Share chain with uncles; pay large miners in coinbase (top N); support transactions (pay all miners with atomic swaps by market makers); market makers buy shares from smaller miners."
- **Share interval: 10 s target** (`IDEAL_BLOCK_TIME = 10`, `sim_overrides.rs`). Share difficulty uses **ASERT** (`pool_difficulty.rs`, `MAX_TARGET_CONSENSUS = 0x1b4188f5`, tuned to sha256d). Stratum jobs are refreshed every 10 s and aim for one share per 3 s per connection (`stratum/work/tracker/mod.rs`).
- **Uncles:** `MAX_UNCLES = 3` (`shares/validation/mod.rs`), `MAX_UNCLES_DEPTH = 3` (`store/dag_store.rs`). Uncle weight: the uncle's miner gets 9/10 of its difficulty and the nephew's miner gets 1/10 (`UNCLE_SCALED_WEIGHT = 9`, `NEPHEW_SCALED_BONUS = 1`).
- **PPLNS window: `PPLNS_WINDOW_SHARES = 120,960`** (about 14 days at 10 s). There is a share-chain **`COINBASE_MATURITY = 6,048`** shares (about 16.8 h) skipped before the window, so `PAYOUT_DEPTH = 127,008` (`accounting/payout/sharechain_pplns/pplns_window.rs`, `docs/architecture/payout-distribution.md`). The distribution is a deterministic function of the chain below the paying share's parent. Every validator rebuilds the coinbase (`validate_bitcoin_payout`).
- **Payouts are in the bitcoin coinbase today.** The README asks for `blockmaxweight=3930000` "to allow for 500 coinbase outputs … before we ship our atomic swap based payout mechanism." The **share chain is also a ledger**: each share carries a share-chain coinbase owned by `miner_address` (`sp2pool1…` bech32m, a taproot key), with its own transactions, wallet and Electrum server (`p2poolv2_wallet`, `p2poolv2_electrum`). Shares are meant to be tradable. Small miners are to be paid by HTLC atomic swaps with market makers (`docs/atomic-swap/htlc_scripts.md`, P2WSH/P2TR HTLCs). That part is **design or WIP, not shipped** as far as I can tell [the swap flow is unverified in code].
- **Share header** (`shares/share_block/mod.rs::ShareHeader`): `prev_share_blockhash`, `uncles`, `miner_bitcoin_address`, `miner_address`, **`bitcoin_header: bitcoin::block::Header`**, `bits`, `time`, donation and fee address and basis points, `bitcoin_height`, `coinbase_proof`. Since v0.16.0 the share commitment is an OP_RETURN in the **last** coinbase output. A `CoinbaseProof` (a SHA256 midstate of the coinbase prefix plus its merkle branch) binds every header field to the PoW. This explicitly stops "one bitcoin header standing behind many share hashes." That is the same bug class as Monero P2Pool's June 2026 exploit (see §3).
- **Mine-on-current-chain rule** (`docs/architecture/mine-on-bitcoin-chain.md`): window shares whose bitcoin prev is not on the paying chain, or one block off it, are weighted zero. Shares carry the bitcoin headers peers need to check this.
- **Status:** testnet4 and signet. The only DNS seed is `seed.testnet4.p2poolv2.org` (`node/dns_seeds.rs`). Mainnet genesis exists in-tree, but I found **no evidence of a live mainnet share chain** [unverified].

**Hydrapool relation (verified in code):** Hydrapool **is built on P2Poolv2's code**. `256foundation/hydrapool/Cargo.toml` depends on `p2poolv2_lib`, `p2poolv2_cli`, `p2poolv2_api` and `bitcoindrpc` from git tag **v0.12.0**, with feature `hydrapool-pplns-accounting`. Since v0.11.2/v0.12.0 (2026-06-12), P2Poolv2 itself has `mode = "hydrapool"`. That is a **standalone (centralized-operator) PPLNS pool, or solo**, with **no share chain**. The ASERT floor is skipped (per-worker vardiff only) and the share commitment is left out of the coinbase. Hydrapool's README: "solo mining and PPLNS accounting", "Payouts are made directly from coinbase", "only accommodate up to 100 users atm for coinbase and block weight reasons", and a mainnet instance at https://test.hydrapool.org. The Hydrapool repo has been quiet since v2.6.0 (2026-06-12). Development seems to have moved into P2Poolv2's `hydrapool` mode [inference]. **Stratum V1: yes** for both, with its own server in `p2poolv2_lib/src/stratum/` (subscribe, authorize, configure/version-rolling, suggest_difficulty, extranonce_subscribe, submit). v0.16.3 reverted the version-rolling mask to BIP320 `1fffe000` because Antminer S19s rejected the BIP323 mask.

**Modularity for XBT (from reading the code):**
- **PoW and header are tightly coupled to `rust-bitcoin` 0.32.** `bitcoin::block::Header` (80 bytes) is embedded in `ShareHeader` and built from Stratum submissions in `stratum/work/difficulty/validate.rs::validate_bitcoin_difficulty`, which uses `header.validate_pow(target)`, i.e. sha256d. The share-target check is `sim_overrides::pow_meets(target, bitcoin_block_hash)`, called from `shares/validation/mod.rs` (about lines 1189 and 1220), with the hash from `bitcoin_header.block_hash()`. About 30 non-test sites construct a `Header`, and 91 of 144 lib files import `bitcoin`. The share's own id (`ShareHeader::block_hash`, sha256d over the share header) is not PoW and can stay.
- Two ways to swap the header: (a) a patched `rust-bitcoin` with a v2 `Header` (164 bytes, BLAKE2b `block_hash`), added through `[patch.crates-io]`. Braidpool did exactly this with its CPUNet rust-bitcoin fork, see §4. (b) A local `XbtHeader` type plus a `PowHash` trait at the about 30 construction sites and the 2–3 PoW gates. Either way: retune ASERT `MAX_TARGET_CONSENSUS` and `MAX_POOL_TARGET` for BLAKE2b difficulty (Knots uses "target shift 22" at the fork), set GBT `rules` (`stratum/work/gbt.rs` sends `["segwit","signet"]`; Knots needs `"blake2b"`), and parse the v2 header fields from GBT.
- **Stratum dialect is the biggest job.** P2Poolv2 serves Bitcoin-style `mining.notify` (coinb1/coinb2 plus merkle branches, a 4-byte nonce, version rolling). BLAKE2b ASICs (Sia-family) speak the **Sia dialect of Stratum V1**. Under header v2 the miner never receives the coinbase: it gets a fixed 35-byte `coinb1` (H2, the first-stage commitment) and grinds `nNonce`/`m_nonce2`/`m_nonce3` plus a 16-byte `m_extranonce` that sits in the header (RATUM README https://github.com/iohzrd/ratum; Knots `src/primitives/block.h` at tag v29.4.1.knots20260508). `stratum/work/notify.rs`, `submit.rs` and `difficulty/validate.rs` need a v2 path. RATUM (Rust, AGPL; read it, don't copy it) and the DATUM BLAKE2b forks are the references.
- The `CoinbaseProof` uses a SHA256 midstate of the coinbase. That still works if txids and the merkle root stay sha256d on XBT, which I believe is true: only the header hash changed. I did not check whether H2 changes how the merkle root is committed [unverified].
- **External templates:** not supported. Each node gets its template from its **own** bitcoind (GBT plus ZMQ `hashblock`). That is already "miner builds own template", the P2Pool model. There is no DATUM-gateway or SV2 Job Declaration input (grep for `datum`, `template provider`, `sv2` finds nothing). A DATUM gateway would need an adapter that takes the gateway's template, or the reverse: P2Poolv2 acting as the "DATUM Prime" that dictates the coinbase split to gateways.
- **Upside for XBT:** header v2 moves the extranonce into the header, so coinbase size no longer reaches the ASIC. The S5-style limit that drove P2Poolv2's 500-output cap and swap plan goes away for v2 miners. The binding limits are block weight (**800,000 WU while RDTS is active, until MTP 2027-09-01**), sigops, and **RDTS's ≤34-byte output scripts**. RATUM's coinbase cap is 1,024 outputs / 32,768 bytes; at about 124 WU per P2WPKH output that is about 127k WU of an 800k block (RATUM README "Coinbase size").

**Verdict:** **best fork base.** It is the only active, permissively licensed Rust share chain with SV1, uncles, ASERT, coinbase PPLNS, deterministic payout validation, formal spec and simulation. Expect to carry a significant patch set for header v2, BLAKE2b and the Sia-dialect SV1.

## 3. Monero P2Pool (SChernykh/p2pool)

Repo: https://github.com/SChernykh/p2pool. C++, GPL-3.0. Last commit 2026-10-24. v4.18.1 (2026-09-22) is current mainnet. v5.0-beta (2026-10-01) is for the FCMP++/Carrot stressnet only (RandomX v2, a new reward split, subaddresses removed).

- **Sidechain params** (`src/side_chain.cpp` constructor; README "Default P2Pool parameters"): main and mini both use `m_targetBlockTime = 10` s, `m_chainWindowSize = 2160` (PPLNS window, "6 hours, auto adjustable"), `m_unclePenalty = 20`%, `UNCLE_BLOCK_DEPTH = 3`, `MAX_UNCLES_PER_BLOCK = 5` (`pool_block.h`). **Nano** uses `m_targetBlockTime = 30` s and `m_unclePenalty = 10`. Mini and nano are separate sidechains with their own consensus IDs and ports (37888 and 37890; main 37889) and a lower min difficulty, sized for small miners. The consensus ID hashes these params, so private chains with custom params are possible (`--sidechain-config`).
- **Payouts:** pure coinbase PPLNS. One output per wallet in the window, with ephemeral pubkeys per output (`SideChain::get_outputs_blob`), 0% fee. Minimum payout ≈ block reward / 2160 ≈ 0.00027 XMR (README). If the pool finds blocks less than once per window, some shares never pay (README note: below about 15 MH/s). Uncles are paid at 80% weight; the penalty goes to the including block.
- **Small miners, new in v5.0-beta** (gist https://gist.github.com/SChernykh/d7e1b24f7d6e1f0627af258a5a0acbe2): payouts are **rounded to multiples of 0.0006 XMR** with probabilistic rounding that is unbiased in expectation. A seed from the latest share ranks who rounds up, and one remainder output goes to the wallet that just missed. The goal is no output below spend cost (about 0.0000294 XMR after FCMP++). The cost is that the smallest mini miner gets paid in 47% of blocks instead of every block. The gist says the main chain is unaffected. **This is directly reusable for XBT dust handling** ("lottery rounding" instead of a dust floor).
- **Verification cost:** every share needs a full RandomX hash. That costs ms and needs a 2 GB dataset or 256 MB light mode. Each share also needs coinbase output reconstruction, which means one key derivation per output. v4.18 (2026-08-05) reports "10% less CPU time spent on block verification" from parallel batch crypto and "up to 100,000 Stratum connections." For XBT a BLAKE2b hash is microseconds, so share verification is dominated by coinbase rebuild and merkle work, not PoW.
- **Incidents:** **GHSA-fm6j-gf38-p925 (critical, 2026-06-13, fixed in v4.16).** A consensus bug let an attacker mine one share and produce about 12,000+ valid-looking copies. Through uncle rewards that captured up to about 80% of a block reward (100% if the attacker also mines the next share). Old and new nodes split onto different sidechains. Advisory: https://github.com/SChernykh/p2pool/security-advisories/GHSA-fm6j-gf38-p925. Release notes: https://github.com/SChernykh/p2pool/releases/tag/v4.16. Secondary reports say mini and nano were exploited first and main around 2026-06-16 [secondary: https://www.merca2.es/2026/06/11/vulnerabilidad-p2pool-monero-robo-recompensas-2394933/; the community "counter-attack" claim is unverified]. **Lesson: every field of the share must be bound to the PoW, and duplicates of the same work must be rejected.** P2Poolv2 v0.16.0 fixed exactly this class. The 2025 Qubic "51%" episode hit Monero itself, not the P2Pool code (https://arxiv.org/abs/2512.01437v2).
- **Modularity:** PoW is in `src/pow_hash.cpp` (RandomX, with seed-hash epochs). The block format is Monero-specific (`pool_block.cpp`, `pool_block_parser.inl`, a CryptoNote tx format, ephemeral keys, Keccak). Its Stratum is the XMRig/Monero JSON dialect, not Bitcoin SV1. Templates come from its own monerod (RPC plus ZMQ). Merge mining (Tari and others) arrived with the 2024-10-12 fork. **Porting to a Bitcoin-format chain means a rewrite**, so it is a design reference only.

## 4. Braidpool

Repo: https://github.com/braidpool/braidpool. The `dev` branch is active (latest 2026-11-06, "fix: fetch GBP bitcoin prices"); `main` was last touched 2025-06-02. No releases. AGPL-3.0. GitHub reports the repo as mostly Jupyter (the simulations), but the node is Rust (`node/`, about 28.5k lines of Rust in the repo) with a TS dashboard.

- **Design** (https://github.com/braidpool/braidpool/blob/dev/docs/overview.md, `braidpool_spec.md`, `braid_consensus.md`): a DAG ("braid") of **beads** (shares). Bead times are targeted at **150 ms–1 s** (`docs/roadmap.md`), with difficulty about 1000× easier than Bitcoin's. **Full Proportional payout over each 2016-block difficulty epoch.** Accumulated rewards are custodied by a **FROST threshold Schnorr multisig** among recent block-finders, so the payout has constant size regardless of miner count. Shares are a transferable UTXO-like asset, building blocks for hashrate derivatives, and a "sub-pools" plan claims 1000× smaller minimum miners.
- **Status:** roadmap "Version 0.0 (current)", with 0.01 "CPUNet mining" in progress. FROST signing (0.3–0.4) and the payout and transaction system (0.5–0.6) are **not implemented**. The node has an SV1 server (`node/src/stratum.rs`, about 5.2k lines: `mining.configure`/`subscribe`/`notify`), an upstream-pool proxy ("audit mode"), and talks to Bitcoin Core over **IPC (Cap'n Proto, multiprocess)** rather than RPC (`docs/braidpool_setup.md`).
- **Modularity:** relevant precedent. CPUNet uses a **modified PoW** through Braidpool's own forks of bitcoin, rust-bitcoin and rust-bitcoincore-rpc (`cpunet` branches). That is exactly the "patch rust-bitcoin" route for BLAKE2b. Templates come from your own node over IPC, the same model as SV2's Template Distribution Protocol.
- **Verdict:** design reference only (DAG, constant-size FROST payout, sub-pools). It is too early to fork, it is AGPL, and it requires sub-second share latency.

## 5. Other current work

### 5a. c2pool (frstrtr): already a BIP-110 BLAKE2b sharechain
https://github.com/frstrtr/c2pool. C++, AGPL-3.0, very active (1,308 commits in 12 weeks, v0.2.9 2026-11-07). It is a C++ reimplementation of forrestv P2Pool (V36 share format) with per-coin binaries, including **`c2pool-bip110`**: a "daemonless" SPV follower that validates byte-exact BLAKE2b on 164-byte v2 headers from height 961,640, has a Stratum-V1 work source, and has an opt-in sharechain (`--bip110-sharechain`, P2P port 9337, Stratum 9336). Params (`src/impl/bip110/params.hpp`) are **copied from BTC P2Pool**: `SHARE_PERIOD = 30`, `CHAIN_LENGTH = 8640`, `SPREAD = 3`, dust 546. It runs live at `bip110.voidbind.com` and is labeled **"Experimental"**. Its README calls the lane "strategic, not revenue." It is the closest existing thing to this project, so read it for the BLAKE2b header and Stratum handling. It is AGPL C++ and uses the old P2Pool design (30 s shares, no uncles), so it is not a Rust fork base. [I did not verify how much hashrate or how many peers its sharechain has.]

### 5b. DATUM and the XBT DATUM ecosystem
- OCEAN DATUM Gateway https://github.com/OCEAN-xyz/datum_gateway (C, MIT, v0.5.1beta 2026-10-19). Templates come from the miner's own node via GBT. It speaks SV1 (with version rolling) to ASICs and the DATUM protocol to the pool ("DATUM Prime"). The pool only dictates the coinbase payout split and cannot replace the template. **DATUM Prime and the protocol spec are not public** ("will be published elsewhere"). OCEAN's README says official DATUM is SHA256d-only and calls Knots' BLAKE2b chain "a hard-forked altcoin".
- XBT side (list: https://github.com/bitcoin-blake/awesome-bitcoin-blake): BLAKE2b gateways from CONVOY (https://github.com/CONVOYMining/datum_gateway), xorpool, MiningMadness, and forks such as https://github.com/NoFlames-bit/datum_blake and https://github.com/gridlabs-science/datum-gateway-blake2b-gridpool. Pools: Lazarus Pool (TIDES), RATUM Prime, Tyger, RIPTIDE (https://github.com/Maveth/tides-pool), CONVOY, Xor Pool. Every one pays from the coinbase.
- **RATUM** https://github.com/iohzrd/ratum (Rust, AGPL-3.0): a Rust DATUM Prime plus a gateway for the Knots BLAKE2b chain, byte-compatible with the C gateway. **Best reference for v2-header Sia-dialect SV1 and coinbase sizing in Rust.** It is AGPL, so read it but don't vendor it into an MIT/Apache fork.
- Fit: a share chain can sit **behind DATUM gateways**. Each gateway builds its own template and asks the "pool" (here, the local share-chain node) for the coinbase split. That keeps miners' template control and reuses gateways the XBT miners already run. The DATUM protocol is reverse-engineered from the C gateway, so treat it as unstable.

### 5c. Stratum V2 / Job Declaration (SRI)
https://github.com/stratum-mining/sv2-apps (Rust, alpha, v0.9.2 2026-11-06) and https://github.com/stratum-mining/stratum (v1.13.0 2026-11-04). Components: pool, jd-server, **jd-client** (miners declare their own templates), a **translator** (SV1 to SV2), and `bitcoin-core-sv2` (Bitcoin Core IPC to the Template Distribution Protocol). SV2's mining messages carry Bitcoin's 80-byte header fields (version, prev_hash, merkle_root, ntime, nbits, nonce), so 164-byte v2 headers would need protocol changes [inferred from the SV2 message layout, not checked against XBT]. Use as a design reference for template declaration only.

### 5d. Hashpool (ecash shares)
- Upstream https://github.com/vnprc/hashpool returns **404** as of 2026-11-08 (GitHub API and web). It may have been renamed, made private or deleted [unverified]. The newest fork I found, https://github.com/wilfredallyn/hashpool (pushed 2026-02-15), carries upstream commits up to **2025-12-12** ("batch translator minting", vnprc). Rust, an SRI fork; license NOASSERTION.
- Design (fork README; https://blog.cashu.space/what-is-hashpool/ via search snippet; Optech https://bitcoinops.org/en/newsletters/2025/01/17/): the pool runs a **CDK Cashu mint**. The SV1-to-SV2 translator proxy attaches a blinded message to each share, and the pool signs it, so each accepted share becomes an **"ehash" token**. Tokens accrue BTC value from blocks found during a limited window, then become redeemable for ecash or BTC. Miners can hold them (luck risk) or sell early. Proof-of-liabilities is proposed (https://gist.github.com/vnprc/a50ecfaf6a55cb3bf52d6ee62d55876a). The **pool custodies the coinbase**, so it is centralized and custodial, with privacy and accountless payouts. Small miners get paid without one coinbase output each. Status: "very early"; demo at pool.hashpool.dev; release v0.1.1 [from search snippet, unverified].
- Relevance: the right idea for **small-miner payout** (tradable claims instead of dust outputs). It conflicts with "no operator", unless a share chain's large outputs fund a federated mint or Lightning swap [design idea, not prior art]. P2Poolv2's "sell shares to market makers via atomic swaps" is the trustless version of the same idea.

### 5e. GridPool (BLAKE2b)
https://github.com/gridlabs-science/gridpool-blake2b (C#, MIT, last 2026-09-04). "Like P2Pool in spirit, but much simpler: miners coordinate on coinbase payout lists instead of maintaining a secondary blockchain." It mentions a 300-output coinbase. Experimental. Useful as a critic FAQ and simulation reference (`docs/critic-faq.md`).

### 5f. Public Pool, ckpool
- Public Pool https://github.com/benjamin-wilson/public-pool (TypeScript, GPL-3.0, last 2026-06-05): a solo SV1 pool, sha256d. Not relevant.
- ckpool https://github.com/ckolivas/ckpool (C, GPL-3.0, active; 2026-10-30 commit moves the solo installer to Bitcoin Core v32.1 because of the IPC mining interface): centralized and solo. Not relevant beyond vardiff (P2Poolv2 already documents its ckpool-derived difficulty in `docs/difficulty_adjustment/ckpool.md`).

### 5g. "P2Pool for Bitcoin" revival in 2025–2026
The live efforts are **P2Poolv2** (testnet4), **c2pool** (multi-coin C++, including BIP-110), and **Braidpool** (research). I found no other active Bitcoin share chain with code [search was limited to GitHub and web].

---

## 6. XBT header v2 facts that shape the design

From Knots `src/primitives/block.h` at tag `v29.4.1.knots20260508` (https://github.com/bitcoinknots/bitcoin/blob/v29.4.1.knots20260508/src/primitives/block.h) and https://github.com/bitcoin-blake/awesome-bitcoin-blake:
- The v2 header is the 80-byte legacy header plus `m_nonce2`(4), `m_nonce3`(4), **`m_extranonce`(16, "Sv1 extranonce")**, `m_time_offset`(4), `m_txcount`(2), `m_flags`(1), `m_xor_key_mask_clear_bits`(1), `m_xor_key`(16), **`m_height`(4)**, **`m_mm_rhs`(32)**, for 164 bytes. v2 is flagged by version bit `0x80000000`. PoW is BLAKE2b-256 with "target shift 22" at the fork; the first BLAKE2b block is 961,640. Testnet4 has its own magic (`1c 16 3f 28`) and ports 48333/48332.
- Because the extranonce is in the header, **coinbase size does not reach the ASIC**. Payout outputs are limited by block weight (800k WU under RDTS until 2027-09-01), sigops, and ≤34-byte scripts.
- `m_mm_rhs` (32 bytes, "merge-mining right-hand side") might carry a share-chain commitment in the header instead of a coinbase OP_RETURN. **[Unverified: I did not read how Knots validates `m_mm_rhs`; check it in the Knots-BLAKE2b source before relying on it.]**
- GBT needs the `"blake2b"` rule.

## 7. Recommendation

**Fork base: P2Poolv2** (https://github.com/p2poolv2/p2poolv2). It is Rust, MIT/Apache (the only permissive candidate, so a fork can be any license), has an SV1 server proven on Antminer/NerdAxe, uses 10 s shares with ASERT, uncles (3/3, 90/10 split), and deterministic coinbase PPLNS validated by every node. It already fixed the Monero-style duplicate-share class (v0.16.0) and is actively maintained. Plugins to write in the fork:
1. A v2 header and BLAKE2b PoW: patch rust-bitcoin (Braidpool CPUNet precedent) or add a local header type at about 30 `Header` sites and the PoW gates in `shares/validation/mod.rs` and `stratum/work/difficulty/validate.rs`. Retune ASERT max target.
2. A Sia-dialect SV1 path in `stratum/work/{notify,submit}` and `difficulty/validate.rs`, using RATUM, CONVOY datum_gateway and c2pool-bip110 as behaviour references.
3. GBT `blake2b` rule plus v2 field parsing (`stratum/work/gbt.rs`, `block_template.rs`).
4. Retune the window and maturity for the XBT hashrate and block rate. 120,960 shares and 14 days are sized for BTC. Use `m_height` and RDTS output limits.
5. Optional: a DATUM-Prime-compatible coinbaser endpoint, so existing XBT DATUM gateways can mine into the share chain.

Expect P2Poolv2 consensus changes upstream (two chain resets in Oct 2026). Pin a tag and rebase deliberately.

**Design references only:** Monero P2Pool (mini/nano split for small miners, probabilistic payout rounding, and the GHSA-fm6j-gf38-p925 lessons), c2pool-bip110 (the first BLAKE2b sharechain; behaviour and KAT reference, AGPL), RATUM and DATUM (template control, coinbase sizing, Sia SV1; AGPL/MIT C), Braidpool (DAG, FROST constant-size payout, sub-pools), Hashpool (ecash shares for small-miner payout), SV2 Job Declaration, and original or jtoomim P2Pool (history and failure modes). Not relevant: Public Pool, ckpool, and Hydrapool as a separate base (use P2Poolv2 directly; Hydrapool is AGPL packaging of `p2poolv2_lib` v0.12.0).

## 8. Could not verify
- vnprc/hashpool's current location and license, and its v0.1.1 release date.
- Whether P2Poolv2 runs a live mainnet share chain (only testnet4 seeds are compiled in).
- Whether P2Poolv2's atomic-swap payout has shipped (only HTLC script docs found).
- Semantics and validation of Knots `m_mm_rhs`, and whether H2 changes merkle-root commitment.
- c2pool-bip110's sharechain participation and hashrate.
- Secondary-source claims about the dates of the June 2026 Monero P2Pool exploit and the counter-attack.
