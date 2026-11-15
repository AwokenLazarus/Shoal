# Shoal testnet4 alpha: mine with us

Shoal is a peer-to-peer mining pool for XBT (Bitcoin on BLAKE2b) with **no pool operator**.
Every miner runs their own node, builds their own block templates, and every block any of us
finds pays all of us straight from its coinbase, by PPLNS over a shared sharechain. Nobody holds
the pool's money, because there is none: the coinbase pays the miners directly.

The alpha runs on **XBT testnet4 only**. Testnet coins have no value. We want to find out
whether Shoal holds up with many independent nodes and miners, not to pay anyone.

What we know so far: [phase 2](phase2-result.md) (three nodes on regtest, attacks included) and
[phase 3](phase3-testnet-result.md) (77 hours on testnet4, one node, one ASIC: 200 blocks, every
coinbase checked to the satoshi). The alpha sharechain started on 2026-11-14 with two nodes and
two CPU miners of ours; its blocks already pay both miners, each coinbase exact. What we don't
know yet is what happens when you join.

## What you need
- A Linux machine (x86_64) with about 20 GB of disk, on all the time while you test. The
  prebuilt binaries need glibc 2.38 or later (Ubuntu 24.04, Debian 13); on anything older, build
  from source.
- An XBT miner that speaks the Sia-profile Stratum XBT ASICs use (for example an Innosilicon
  S11), or just a CPU: testnet4 shares start at difficulty 1, and testnet4 lets anyone mine a
  block 20 minutes after the last one.

## 1. Run an XBT testnet4 node
XBT runs on Bitcoin Knots with the BLAKE2b fork, release **29.4.2.knots20260508** or later from
[bitcoinknots.org](https://bitcoinknots.org/files/29.x/). Verify the download against its
`SHA256SUMS` and signature.

Use [`testnet4/bitcoin.conf`](testnet4/bitcoin.conf) (set a real `rpcpassword`), start
`bitcoind`, and wait for it to sync (`bitcoin-cli -testnet4 getblockchaininfo`: `blocks` equal to
`headers`, about 15 GB). Post-fork block headers are 164 bytes: `getblockheader <hash> false` is
328 hex characters.

## 2. Make two addresses
`p2poolv2_cli` comes with the Shoal binaries of step 3.
```sh
bitcoin-cli -testnet4 createwallet shoal
bitcoin-cli -testnet4 getnewaddress "" bech32        # your payout address, tb1q...
T=$(bitcoin-cli -testnet4 getnewaddress "" bech32m)   # for your share address
bitcoin-cli -testnet4 getaddressinfo "$T" | grep witness_program
p2poolv2_cli address -n testnet4 <witness_program>   # your share address, tp2pool1...
```
The payout address receives your part of every block. The share address names you on the
sharechain.

## 3. Run a Shoal node
Prebuilt Linux binaries are on the [releases page of the Shoal fork of P2Poolv2](https://github.com/AwokenLazarus/p2poolv2/releases)
(`p2poolv2`, `p2poolv2_cli` and the CPU miner `shoal-grind`; check `SHA256SUMS`). Or build them:
```sh
sudo apt install build-essential clang cmake pkg-config libzmq3-dev libssl-dev git curl
curl https://sh.rustup.rs -sSf | sh        # rustup picks the pinned Rust version
git clone -b shoal/xbt-header https://github.com/AwokenLazarus/p2poolv2
cd p2poolv2 && cargo build --release -p p2poolv2_node -p p2poolv2_cli
sudo cp target/release/p2poolv2 target/release/p2poolv2_cli /usr/local/bin/
```
Copy [`testnet4/config.toml`](testnet4/config.toml) to `/etc/shoal/config.toml` and fill in the
lines marked `EDIT`: your payout address and the RPC login from `bitcoin.conf`. Start it (a systemd unit is in [`testnet4/shoal.service`](testnet4/shoal.service)).

**Bootstrap peer:** `/dns4/shoal-testnet.lazarus-xbt.xyz/tcp/6884` (already in the template).
The same host runs an XBT testnet4 node on port 48333 (already in `bitcoin.conf`). Names in
`dial_peers` need release alpha-1 or later; upstream P2Poolv2 takes IP addresses only.

The sharechain's rules are built into the binary: genesis 2026-11-14 04:00 UTC, share floor
difficulty 1, a PPLNS window of 8,640 shares (one day at 10 s shares) and a maturity of 360
shares (one hour). Every node must run the same alpha release; a node on other rules sees a
different genesis and won't peer.

Check it: `p2poolv2_cli -c /etc/shoal/config.toml info` shows the chain tip, and the log says
when it connects to peers and syncs the sharechain.

## 4. Point your miner at your node
- URL: `stratum+tcp://<your node's LAN address>:3333`
- User: `<your payout address>.<worker name>`
- Password: `p2p=<your share address>` (add `,d=1` for a CPU miner)

Keep the Stratum port on your own network: it is for your miners, not for the public.

A CPU miner for testing comes with the release (`shoal-grind`; source in this repo under
`crates/shoal-grind`): `shoal-grind --host <node> --port 3333 --threads 2 --user <payout
address>.cpu --pass 'p2p=<share address>,d=1'`. At difficulty 1 one CPU thread finds a share
every 10 to 20 minutes. The share difficulty follows the whole sharechain's hashrate (one share
per 10 s): while an ASIC is mining on it, a CPU finds shares rarely, though its node still follows
and checks the chain.

## 5. Check your payouts yourself
A block pays the shares of the PPLNS window except the newest 360 (the maturity: an hour at
10 s shares, longer while few miners are on and shares are slow). While the sharechain is
shorter than that, as it is in the alpha's first days, nothing is skipped and your shares count
at once. Every Shoal block pays you your part of the window, whoever finds it.

Don't take a node's word for it: [`e2e/verify_pplns.py`](e2e/verify_pplns.py) recomputes every
block's payout from the sharechain and the XBT chain alone:
```sh
cp -a /var/lib/shoal/store.db /tmp/shoal-copy      # the checker reads a copy
python3 e2e/verify_pplns.py --network testnet4 --jobs 3 \
  --cli /usr/local/bin/p2poolv2_cli --db /tmp/shoal-copy --api /etc/shoal/config.toml \
  --bitcoin-cli "$(command -v bitcoin-cli)" --datadir ~/.bitcoin
```
Every line should end in `OK`. A `MISMATCH` is exactly what we want to hear about.

## What to report
Open an issue on [the Shoal repository](https://github.com/AwokenLazarus/Shoal/issues) with
your node's log around the time, the output of `p2poolv2_cli info`, and the checker's output if
it complains. Useful in particular: a `MISMATCH`, `SHARECHAIN REORG ALARM` in your log, your
node stuck behind others, or a miner whose shares are rejected.

## Good to know
- The alpha sharechain may be reset (a new release with a new genesis) when we change a rule.
  We will say so in advance.
- Your node is a public peer: other nodes see its IP address. Your Stratum port should not be.
- Withholding blocks can't be prevented by any pool; Shoal shows it instead
  (`curl localhost:46884/withholding`), per miner.
- This is a fork of [P2Poolv2](https://github.com/p2poolv2/p2poolv2), not affiliated with or
  endorsed by its developers; report Shoal problems to Shoal.
