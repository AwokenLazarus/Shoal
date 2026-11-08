// SPDX-License-Identifier: MIT OR Apache-2.0
//! Writes a deterministic XBT anchor block for Shoal's sharechain genesis on a test network.
//!
//! Upstream anchors its regtest and signet sharechains to Bitcoin's genesis blocks, whose 80-byte
//! v1 headers XBT code refuses. This builds the v2 equivalent: that network's Bitcoin genesis
//! coinbase, a v2 header committing to it (the genesis time and bits, height 0, one
//! transaction), BLAKE2b-mined to its own target by incrementing the nonce from zero. Same
//! output every run.
//!
//! Not for testnet4 or mainnet: those need a real XBT block as their anchor.
//!
//! Usage: `cargo run -p shoal-header --example regtest_genesis -- <out.dat> [regtest|signet]`

use bitcoin::blockdata::constants::genesis_block;
use bitcoin::consensus::serialize;
use bitcoin::hashes::Hash;
use bitcoin::{BlockHash, Network, Target, TxMerkleNode};
use shoal_header::{V2Fields, XbtBlock, XbtHeader};

fn main() {
    let out = std::env::args()
        .nth(1)
        .expect("usage: regtest_genesis <out.dat> [regtest|signet]");
    let network = match std::env::args().nth(2).as_deref() {
        None | Some("regtest") => Network::Regtest,
        Some("signet") => Network::Signet,
        Some(other) => panic!("unsupported network {other}: regtest or signet"),
    };
    let btc = genesis_block(network);
    let coinbase = btc.txdata[0].clone();
    let mut header = XbtHeader {
        version: btc.header.version,
        prev_blockhash: BlockHash::all_zeros(),
        merkle_root: TxMerkleNode::from_raw_hash(coinbase.compute_txid().to_raw_hash()),
        time: btc.header.time,
        bits: btc.header.bits,
        nonce: 0,
        v2: V2Fields {
            height: 0,
            txcount: 1,
            ..V2Fields::default()
        },
    };
    assert!(header.grind(Target::from_compact(header.bits), u32::MAX));
    let block = XbtBlock {
        header,
        txdata: vec![coinbase],
    };
    assert!(block.check_merkle_root());
    let bytes = serialize(&block);
    std::fs::write(&out, &bytes).expect("write");
    println!(
        "{} {} bytes nonce={} hash={}",
        out,
        bytes.len(),
        block.header.nonce,
        block.block_hash()
    );
}
