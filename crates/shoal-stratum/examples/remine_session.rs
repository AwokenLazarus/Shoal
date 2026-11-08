// SPDX-License-Identifier: MIT OR Apache-2.0
//! Re-mines a recorded stratum session's submission for XBT (test fixtures only).
//!
//! Given a session's `template.json`, its job coinbase halves (`notify.json`, P2Poolv2 layout) and
//! extranonce1, it builds the XBT job exactly as Shoal's fork does (zero-extranonce coinbase, no
//! template branches, header from the template) and searches extranonce2/nonce, multi-threaded,
//! for a share whose true difficulty is at least `--min-difficulty`. Prints the Sia-dialect
//! `extranonce2 ntime nonce` hex triple.
//!
//! `cargo run --release -p shoal-stratum --example remine_session -- <session dir> <min difficulty> [threads]`

use bitcoin::consensus::Decodable;
use bitcoin::hashes::Hash;
use bitcoin::{BlockHash, CompactTarget, TxMerkleNode};
use lazarus_protocol::pow::{asic_hash, prevblock_hidden};
use serde_json::Value;
use shoal_stratum::{apply_submission, job_header, JobTemplate, Submission};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

fn read(dir: &str, file: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(format!("{dir}/{file}")).unwrap()).unwrap()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = args.get(1).expect("session dir").clone();
    let min_diff: f64 = args.get(2).expect("min difficulty").parse().unwrap();
    let threads: u64 = args.get(3).map(|t| t.parse().unwrap()).unwrap_or(4);

    let t = read(&dir, "template.json");
    let n = read(&dir, "notify.json");
    let a = read(&dir, "authorize_response.json");
    let p = n["params"].as_array().unwrap();
    let coinbase_hex = format!(
        "{}{}{}",
        p[2].as_str().unwrap(),
        "00".repeat(12),
        p[3].as_str().unwrap()
    );
    let coinbase =
        bitcoin::Transaction::consensus_decode(&mut hex::decode(coinbase_hex).unwrap().as_slice())
            .unwrap();
    let raw_version = t["version"].as_i64().unwrap() as u32 & !0x8000_0000;
    let job = job_header(&JobTemplate {
        version: raw_version as i32,
        prev_blockhash: BlockHash::from_str(t["previousblockhash"].as_str().unwrap()).unwrap(),
        merkle_root: TxMerkleNode::from_raw_hash(coinbase.compute_txid().to_raw_hash()),
        curtime: t["curtime"].as_u64().unwrap() as u32,
        bits: CompactTarget::from_unprefixed_hex(t["bits"].as_str().unwrap()).unwrap(),
        height: t["height"].as_u64().unwrap() as u32,
        txcount: (t["transactions"].as_array().unwrap().len() + 1) as u16,
    });
    let en1: [u8; 4] = hex::decode(a["result"][1].as_str().unwrap())
        .unwrap()
        .try_into()
        .unwrap();
    // Share target for the difficulty: 0xffff·2^208 / d, as its top 64 bits.
    let target_hi = ((0xffffu64 as f64) * 65536.0 / min_diff) as u64;

    let done = Arc::new(AtomicBool::new(false));
    let found = Arc::new(Mutex::new(None));
    let mut handles = Vec::new();
    for thread in 0..threads {
        let (done, found) = (Arc::clone(&done), Arc::clone(&found));
        handles.push(std::thread::spawn(move || {
            let mut en2 = thread;
            let prev = prevblock_hidden(&job.prev_blockhash.to_byte_array());
            while !done.load(Ordering::Relaxed) {
                // hash1 is fixed per extranonce2; only the 80-byte pass runs per nonce, as on an
                // ASIC. No XOR key is set, so the pass's hash is the PoW hash.
                let base = Submission {
                    extranonce2: en2.to_le_bytes(),
                    ntime: [0; 8],
                    nonce: [0; 8],
                };
                let hash1 = apply_submission(&job, &en1, &base).to_lazarus().hash1();
                for nonce in 0u32..(1 << 22) {
                    if nonce & 0xffff == 0 && done.load(Ordering::Relaxed) {
                        return;
                    }
                    let h = asic_hash(&prev, &hash1, nonce, 0, 0, 0);
                    if u64::from_be_bytes(h[..8].try_into().unwrap()) < target_hi {
                        let mut nn = [0u8; 8];
                        nn[..4].copy_from_slice(&nonce.to_le_bytes());
                        let sub = Submission {
                            extranonce2: en2.to_le_bytes(),
                            ntime: [0; 8],
                            nonce: nn,
                        };
                        // Cross-check the fast path against the full header hash.
                        assert_eq!(apply_submission(&job, &en1, &sub).pow_hash_be(), h);
                        done.store(true, Ordering::Relaxed);
                        *found.lock().unwrap() = Some((sub, h));
                        return;
                    }
                }
                en2 += threads;
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    let (sub, h) = found.lock().unwrap().take().unwrap();
    let hash_value = u128::from_be_bytes(h[..16].try_into().unwrap()) as f64 * 2f64.powi(128);
    eprintln!(
        "difficulty {:.3}",
        (0xffff as f64) * 2f64.powi(208) / hash_value
    );
    println!(
        "{} {} {}",
        hex::encode(sub.extranonce2),
        hex::encode(sub.ntime),
        hex::encode(sub.nonce)
    );
}
