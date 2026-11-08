// SPDX-License-Identifier: MIT OR Apache-2.0
//! shoal-grind: a CPU miner for XBT's Sia-dialect Stratum V1, for regtest and end-to-end tests.
//!
//! It works only from what the pool sends (`mining.notify`'s prevhash, coinb1 and nbits), the
//! way a Sia-profile ASIC does, so it also checks that a server's jobs are minable:
//! `hash1 = blake2b(0x00 ‖ coinb1 ‖ en1 ‖ en2)`, then the 80-byte pass over
//! `prev_hidden ‖ nonce ‖ nonce2 ‖ time_offset ‖ nonce3 ‖ hash1`.
//!
//! Not for real mining: a CPU finds nothing on mainnet.
//!
//! Usage: `shoal-grind --host H --port P --user U [--pass P] [--threads N] [--secs S]`

use lazarus_protocol::pow::{asic_hash, blake2b_256};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone)]
struct Job {
    id: String,
    prev: [u8; 32],
    coinb1: Vec<u8>,
}

struct Shared {
    job: Mutex<Option<Job>>,
    job_seq: AtomicU64,
    /// Top 64 bits of the share target (target = 0xffff·2^208 / difficulty).
    target_hi: AtomicU64,
    en1: Mutex<Vec<u8>>,
    hashes: AtomicU64,
    stop: AtomicBool,
    out: Mutex<TcpStream>,
    next_id: AtomicU64,
    user: String,
}

fn target_hi_for(difficulty: f64) -> u64 {
    let d = if difficulty > 0.0 { difficulty } else { 1.0 };
    let t = (0xffffu64 as f64) * 65536.0 / d;
    if t >= u64::MAX as f64 {
        u64::MAX
    } else {
        t as u64
    }
}

fn send(shared: &Shared, v: &Value) {
    let mut s = shared.out.lock().unwrap();
    let _ = writeln!(s, "{v}");
}

fn worker(shared: Arc<Shared>, thread: u32) {
    let mut en2_counter: u64 = 0;
    let mut current_seq = u64::MAX;
    let mut job: Option<Job> = None;
    let mut en1 = Vec::new();
    while !shared.stop.load(Ordering::Relaxed) {
        let seq = shared.job_seq.load(Ordering::Acquire);
        if seq != current_seq {
            job = shared.job.lock().unwrap().clone();
            en1 = shared.en1.lock().unwrap().clone();
            current_seq = seq;
        }
        let Some(j) = job.as_ref() else {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        };
        // Each thread owns the top byte of extranonce2, so threads never repeat work.
        en2_counter = en2_counter.wrapping_add(1);
        let mut en2 = en2_counter.to_le_bytes();
        en2[7] = thread as u8;
        let mut leaf = Vec::with_capacity(1 + j.coinb1.len() + 12);
        leaf.push(0u8);
        leaf.extend_from_slice(&j.coinb1);
        leaf.extend_from_slice(&en1);
        leaf.extend_from_slice(&en2);
        let hash1 = blake2b_256(&leaf);
        let target_hi = shared.target_hi.load(Ordering::Relaxed);
        for nonce in 0u32..(1 << 20) {
            let h = asic_hash(&j.prev, &hash1, nonce, 0, 0, 0);
            let hi = u64::from_be_bytes(h[..8].try_into().unwrap());
            if hi < target_hi {
                let id = shared.next_id.fetch_add(1, Ordering::Relaxed);
                let mut nn = [0u8; 8];
                nn[..4].copy_from_slice(&nonce.to_le_bytes());
                send(
                    &shared,
                    &json!({"id": id, "method": "mining.submit", "params": [
                        shared.user, j.id, hex::encode(en2), hex::encode([0u8; 8]), hex::encode(nn)
                    ]}),
                );
                eprintln!(
                    "share job={} en2={} nonce={nonce:08x} hash={}",
                    j.id,
                    hex::encode(en2),
                    hex::encode(&h[..8])
                );
            }
            if nonce & 0xffff == 0xffff {
                shared.hashes.fetch_add(0x10000, Ordering::Relaxed);
                if shared.job_seq.load(Ordering::Relaxed) != current_seq
                    || shared.stop.load(Ordering::Relaxed)
                {
                    break;
                }
            }
        }
    }
}

fn main() {
    let mut host = "127.0.0.1".to_string();
    let mut port: u16 = 3333;
    let mut user = String::new();
    let mut pass = "x".to_string();
    let mut threads: u32 = 2;
    let mut secs: u64 = 600;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut val = || args.next().unwrap_or_else(|| panic!("{a} needs a value"));
        match a.as_str() {
            "--host" => host = val(),
            "--port" => port = val().parse().expect("port"),
            "--user" => user = val(),
            "--pass" => pass = val(),
            "--threads" => threads = val().parse().expect("threads"),
            "--secs" => secs = val().parse().expect("secs"),
            _ => {
                eprintln!("usage: shoal-grind --host H --port P --user U [--pass P] [--threads N] [--secs S]");
                std::process::exit(2);
            }
        }
    }
    assert!(!user.is_empty(), "--user is required");
    assert!((1..=64).contains(&threads), "--threads 1..64");

    let stream = TcpStream::connect((host.as_str(), port)).expect("connect");
    let shared = Arc::new(Shared {
        job: Mutex::new(None),
        job_seq: AtomicU64::new(0),
        target_hi: AtomicU64::new(target_hi_for(1.0)),
        en1: Mutex::new(Vec::new()),
        hashes: AtomicU64::new(0),
        stop: AtomicBool::new(false),
        out: Mutex::new(stream.try_clone().expect("clone")),
        next_id: AtomicU64::new(10),
        user: user.clone(),
    });
    send(
        &shared,
        &json!({"id": 1, "method": "mining.subscribe", "params": ["shoal-grind/0.1"]}),
    );
    send(
        &shared,
        &json!({"id": 2, "method": "mining.authorize", "params": [user, pass]}),
    );

    for t in 0..threads {
        let s = Arc::clone(&shared);
        std::thread::spawn(move || worker(s, t));
    }
    {
        let s = Arc::clone(&shared);
        std::thread::spawn(move || {
            let start = Instant::now();
            loop {
                std::thread::sleep(Duration::from_secs(10));
                let h = s.hashes.load(Ordering::Relaxed) as f64;
                eprintln!("rate {:.1} MH/s", h / start.elapsed().as_secs_f64() / 1e6);
                if start.elapsed().as_secs() >= secs {
                    eprintln!("time limit reached");
                    s.stop.store(true, Ordering::Relaxed);
                    std::process::exit(0);
                }
            }
        });
    }

    let reader = BufReader::new(stream);
    let (mut accepted, mut rejected) = (0u64, 0u64);
    for line in reader.lines() {
        let Ok(line) = line else { break };
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            eprintln!("unparsed: {line}");
            continue;
        };
        match msg.get("method").and_then(Value::as_str) {
            Some("mining.notify") => {
                let p = msg["params"].as_array().cloned().unwrap_or_default();
                let prev: Option<[u8; 32]> = p
                    .get(1)
                    .and_then(Value::as_str)
                    .and_then(|s| hex::decode(s).ok())
                    .and_then(|v| v.try_into().ok());
                let coinb1 = p
                    .get(2)
                    .and_then(Value::as_str)
                    .and_then(|s| hex::decode(s).ok());
                let (Some(prev), Some(coinb1)) = (prev, coinb1) else {
                    eprintln!("bad notify: {line}");
                    continue;
                };
                if coinb1.len() != 39 {
                    eprintln!(
                        "notify coinb1 is {} bytes, not the 39-byte Sia form",
                        coinb1.len()
                    );
                }
                let seq = shared.job_seq.load(Ordering::Relaxed) + 1;
                let id = p.first().and_then(Value::as_str).unwrap_or("").to_string();
                eprintln!("job {id}");
                *shared.job.lock().unwrap() = Some(Job { id, prev, coinb1 });
                shared.job_seq.store(seq, Ordering::Release);
            }
            Some("mining.set_difficulty") => {
                let d = msg["params"][0].as_f64().unwrap_or(1.0);
                eprintln!("difficulty {d}");
                shared.target_hi.store(target_hi_for(d), Ordering::Relaxed);
            }
            Some(other) => eprintln!("ignored method {other}"),
            None => {
                let id = msg.get("id").and_then(Value::as_u64).unwrap_or(0);
                if id == 1 {
                    if let Some(en1) = msg["result"][1].as_str().and_then(|s| hex::decode(s).ok()) {
                        eprintln!("subscribed en1={}", hex::encode(&en1));
                        *shared.en1.lock().unwrap() = en1;
                    }
                } else if id == 2 {
                    eprintln!("authorize: {}", msg["result"]);
                } else if msg["result"] == Value::Bool(true) {
                    accepted += 1;
                    eprintln!("accepted ({accepted} ok / {rejected} rejected)");
                } else {
                    rejected += 1;
                    eprintln!(
                        "rejected: {} ({accepted} ok / {rejected} rejected)",
                        msg["error"]
                    );
                }
            }
        }
    }
    eprintln!("connection closed");
}
