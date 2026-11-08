// SPDX-License-Identifier: MIT OR Apache-2.0
//! Writes the deterministic XBT regtest anchor block Shoal's sharechain genesis uses.
//!
//! Bitcoin's regtest genesis is an 80-byte v1 header, which XBT code refuses. This builds the
//! v2 equivalent: Bitcoin's regtest genesis coinbase, a v2 header committing to it (time and
//! bits as Bitcoin's regtest genesis, height 0, one transaction), BLAKE2b-mined to the regtest
//! target by incrementing the nonce from zero. Same output every run.
//!
//! Usage: `cargo run -p shoal-header --example regtest_genesis -- <out.dat>`

use bitcoin::blockdata::constants::genesis_block;
use bitcoin::consensus::serialize;
use bitcoin::hashes::Hash;
use bitcoin::{BlockHash, Network, Target, TxMerkleNode};
use shoal_header::{V2Fields, XbtBlock, XbtHeader};

fn main() {
    let out = std::env::args()
        .nth(1)
        .expect("usage: regtest_genesis <out.dat>");
    let btc = genesis_block(Network::Regtest);
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
    assert!(header.grind(Target::from_compact(header.bits), 1 << 20));
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
