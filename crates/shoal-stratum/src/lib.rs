// SPDX-License-Identifier: MIT OR Apache-2.0
//! The Stratum V1 dialect that XBT (BLAKE2b) ASICs speak, for Shoal's sharechain node.
//!
//! XBT ASICs are Sia-profile miners. Compared with Bitcoin's Stratum:
//!
//! - **subscribe:** the same shape, with a 4-byte extranonce1 and an 8-byte extranonce2.
//! - **notify:** `coinb1` is `000000 ‖ h2 ‖ 00000000`, where `h2` commits to every header field
//!   above the extranonce (merkle root included). `coinb2` and the merkle list are empty, the
//!   version is empty, `nbits` is in header byte order, and `ntime` is 8 zero bytes the miner may
//!   roll.
//! - **submit:** `[user, job_id, extranonce2 (8 B), ntime (8 B), nonce (8 B)]`. The 8-byte
//!   "ntime" is `time_offset ‖ nonce3`, and the 8-byte "nonce" is `nonce ‖ nonce2`, all
//!   little-endian.
//!
//! The extranonce lives in the **header**, not the coinbase. So a job's coinbase is fixed: Shoal
//! builds it with the extranonce bytes zero, and every miner on the job hashes the same merkle
//! root. The miner computes `hash1 = blake2b(0x00 ‖ coinb1 ‖ en1 ‖ en2)`, which equals the
//! header's `hash1` when the header extranonce is `0000 ‖ en1 ‖ en2`.
//!
//! The byte layout follows `lazarus-protocol` (Lazarus Pool's gateway), which mines on mainnet.

use bitcoin::block::Version;
use bitcoin::hashes::Hash;
use bitcoin::pow::CompactTarget;
use bitcoin::{BlockHash, TxMerkleNode};
use lazarus_protocol::pow::{header_extranonce, prevblock_hidden};
use serde_json::{json, Value};
use shoal_header::{V2Fields, XbtHeader};

/// Bytes of extranonce1 per session.
pub const EXTRANONCE1_SIZE: usize = 4;
/// Bytes of extranonce2 the miner rolls.
pub const EXTRANONCE2_SIZE: usize = 8;
/// Bytes the coinbase scriptSig reserves for the extranonce in Bitcoin's dialect. On XBT they
/// stay zero, so the coinbase (and the merkle root) is the same for every miner on a job.
pub const COINBASE_EXTRANONCE_FILL: usize = EXTRANONCE1_SIZE + EXTRANONCE2_SIZE;

/// A job's header fields from a block template, before any miner's nonces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobTemplate {
    pub version: i32,
    /// Previous block hash, rust-bitcoin internal order (as parsed from GBT's display hex).
    pub prev_blockhash: BlockHash,
    pub merkle_root: TxMerkleNode,
    pub curtime: u32,
    pub bits: CompactTarget,
    /// Height of the block being mined.
    pub height: u32,
    /// Transactions in the block, coinbase included.
    pub txcount: u16,
}

/// The job header every miner on this job starts from: extranonce and nonces zero, no XOR key.
///
/// Shoal sets no `xor_key`: the mask exists so a pool can hide which shares are blocks from its
/// hashers. On a sharechain the miner builds its own template, so there is nobody to hide from.
pub fn job_header(t: &JobTemplate) -> XbtHeader {
    XbtHeader {
        version: Version::from_consensus(t.version),
        prev_blockhash: t.prev_blockhash,
        merkle_root: t.merkle_root,
        time: t.curtime,
        bits: t.bits,
        nonce: 0,
        v2: V2Fields {
            height: t.height as i32,
            txcount: t.txcount,
            ..V2Fields::default()
        },
    }
}

/// `coinb1` for a Sia-profile miner: `000000 ‖ h2 ‖ 00000000`, as hex.
pub fn coinb1_hex(header: &XbtHeader) -> String {
    hex::encode(header.to_lazarus().coinb1_sia())
}

/// The `prevhash` notify field: the previous hash as the ASIC's 80-byte pass takes it.
pub fn prevhash_hex(header: &XbtHeader) -> String {
    hex::encode(prevblock_hidden(&header.prev_blockhash.to_byte_array()))
}

/// The `nbits` notify field: bits in header byte order.
pub fn nbits_hex(header: &XbtHeader) -> String {
    hex::encode(header.bits.to_consensus().to_le_bytes())
}

/// The full `mining.notify` message for a job.
pub fn notify_message(job_id: &str, header: &XbtHeader, clean_jobs: bool) -> Value {
    json!({
        "id": null,
        "method": "mining.notify",
        "params": [
            job_id,
            prevhash_hex(header),
            coinb1_hex(header),
            "",
            [],
            "",
            nbits_hex(header),
            hex::encode([0u8; 8]),
            clean_jobs,
        ]
    })
}

/// Why a submission could not be applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmitError {
    /// A field is missing, not hex, or not exactly 8 bytes.
    BadField(&'static str),
}

impl std::fmt::Display for SubmitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SubmitError::BadField(name) => write!(f, "bad {name}: expected 16 hex characters"),
        }
    }
}

impl std::error::Error for SubmitError {}

/// The miner-chosen parts of a `mining.submit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Submission {
    pub extranonce2: [u8; 8],
    /// `time_offset ‖ nonce3`, little-endian.
    pub ntime: [u8; 8],
    /// `nonce ‖ nonce2`, little-endian.
    pub nonce: [u8; 8],
}

fn field8(hex_str: Option<&str>, name: &'static str) -> Result<[u8; 8], SubmitError> {
    let s = hex_str.ok_or(SubmitError::BadField(name))?;
    if s.len() != 16 {
        return Err(SubmitError::BadField(name));
    }
    let v = hex::decode(s).map_err(|_| SubmitError::BadField(name))?;
    v.try_into().map_err(|_| SubmitError::BadField(name))
}

impl Submission {
    /// Parse `mining.submit` params 2, 3 and 4 (extranonce2, ntime, nonce). Each must be exactly
    /// 16 hex characters; anything else is refused rather than padded.
    pub fn parse(
        extranonce2: Option<&str>,
        ntime: Option<&str>,
        nonce: Option<&str>,
    ) -> Result<Self, SubmitError> {
        Ok(Submission {
            extranonce2: field8(extranonce2, "extranonce2")?,
            ntime: field8(ntime, "ntime")?,
            nonce: field8(nonce, "nonce")?,
        })
    }
}

/// The header a miner hashed: the job header with this session's extranonce1 and the
/// submission's extranonce2, nonces and rolled ntime.
pub fn apply_submission(
    job: &XbtHeader,
    extranonce1: &[u8; EXTRANONCE1_SIZE],
    sub: &Submission,
) -> XbtHeader {
    let mut h = *job;
    let mut en12 = [0u8; COINBASE_EXTRANONCE_FILL];
    en12[..EXTRANONCE1_SIZE].copy_from_slice(extranonce1);
    en12[EXTRANONCE1_SIZE..].copy_from_slice(&sub.extranonce2);
    h.v2.extranonce = header_extranonce(&en12);
    let le = |b: &[u8]| u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    h.nonce = le(&sub.nonce[0..4]);
    h.v2.nonce2 = le(&sub.nonce[4..8]);
    h.v2.time_offset = le(&sub.ntime[0..4]);
    h.v2.nonce3 = le(&sub.ntime[4..8]);
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::pow::Target;
    use lazarus_protocol::pow::{asic_hash, blake2b_256, xor_mask};

    fn template() -> JobTemplate {
        JobTemplate {
            version: 0x2000_0000,
            prev_blockhash: BlockHash::from_byte_array([0x11; 32]),
            merkle_root: TxMerkleNode::from_byte_array([0x22; 32]),
            curtime: 1_800_000_000,
            bits: CompactTarget::from_consensus(0x207f_ffff),
            height: 1234,
            txcount: 3,
        }
    }

    /// What a Sia-profile ASIC computes from a notify and its own nonces: the merkle "leaf"
    /// `blake2b(0x00 ‖ coinb1 ‖ en1 ‖ en2)`, then the 80-byte pass.
    fn miner_pow(notify: &Value, en1: &[u8; 4], sub: &Submission) -> [u8; 32] {
        let p = notify["params"].as_array().unwrap();
        let prev: [u8; 32] = hex::decode(p[1].as_str().unwrap())
            .unwrap()
            .try_into()
            .unwrap();
        let coinb1 = hex::decode(p[2].as_str().unwrap()).unwrap();
        assert_eq!(p[3], "", "coinb2 is empty");
        assert_eq!(p[4].as_array().unwrap().len(), 0, "no merkle branches");
        let mut leaf = vec![0u8];
        leaf.extend_from_slice(&coinb1);
        leaf.extend_from_slice(en1);
        leaf.extend_from_slice(&sub.extranonce2);
        let hash1 = blake2b_256(&leaf);
        let le = |b: &[u8]| u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        asic_hash(
            &prev,
            &hash1,
            le(&sub.nonce[0..4]),
            le(&sub.nonce[4..8]),
            le(&sub.ntime[0..4]),
            le(&sub.ntime[4..8]),
        )
    }

    fn sample_submission() -> Submission {
        Submission::parse(
            Some("0102030405060708"),
            Some("0a0000000b000000"),
            Some("deadbeefcafef00d"),
        )
        .unwrap()
    }

    #[test]
    fn rebuilt_header_hashes_to_what_the_miner_computed() {
        let job = job_header(&template());
        let notify = notify_message("00000000000000ab", &job, true);
        let en1 = [0xaa, 0xbb, 0xcc, 0xdd];
        let sub = sample_submission();
        let header = apply_submission(&job, &en1, &sub);
        // No XOR key, so the PoW hash is the miner's 80-byte-pass hash itself.
        assert_eq!(xor_mask(&header.v2.xor_key, 0), [0u8; 32]);
        assert_eq!(header.pow_hash_be(), miner_pow(&notify, &en1, &sub));
    }

    #[test]
    fn each_submitted_field_lands_in_the_header() {
        let job = job_header(&template());
        let header = apply_submission(&job, &[1, 2, 3, 4], &sample_submission());
        assert_eq!(header.nonce, 0xefbe_adde);
        assert_eq!(header.v2.nonce2, 0x0df0_feca);
        assert_eq!(header.v2.time_offset, 0x0a);
        assert_eq!(header.v2.nonce3, 0x0b);
        assert_eq!(&header.v2.extranonce[..4], &[0, 0, 0, 0]);
        assert_eq!(&header.v2.extranonce[4..8], &[1, 2, 3, 4]);
        assert_eq!(&header.v2.extranonce[8..], &[1, 2, 3, 4, 5, 6, 7, 8]);
        // Job fields are untouched.
        assert_eq!(header.merkle_root, job.merkle_root);
        assert_eq!(header.time, job.time);
        assert_eq!(header.v2.height, 1234);
        assert_eq!(header.v2.txcount, 3);
        assert_eq!(header.v2.flags, 0);
    }

    #[test]
    fn different_extranonce1_gives_different_work() {
        let job = job_header(&template());
        let sub = sample_submission();
        let a = apply_submission(&job, &[0, 0, 0, 1], &sub);
        let b = apply_submission(&job, &[0, 0, 0, 2], &sub);
        assert_ne!(a.block_hash(), b.block_hash());
    }

    #[test]
    fn notify_fields_have_the_dialect_shape() {
        let job = job_header(&template());
        let notify = notify_message("j1", &job, false);
        let p = notify["params"].as_array().unwrap();
        assert_eq!(p.len(), 9);
        let coinb1 = p[2].as_str().unwrap();
        assert_eq!(coinb1.len(), 78, "39 bytes");
        assert!(coinb1.starts_with("000000") && coinb1.ends_with("00000000"));
        assert_eq!(p[6], "ffff7f20", "bits 0x207fffff in header byte order");
        assert_eq!(p[7], "0000000000000000");
        assert_eq!(p[8], false);
    }

    #[test]
    fn merkle_root_and_height_are_committed_by_coinb1() {
        let job = job_header(&template());
        let mut other = template();
        other.merkle_root = TxMerkleNode::from_byte_array([0x23; 32]);
        assert_ne!(coinb1_hex(&job), coinb1_hex(&job_header(&other)));
        let mut other = template();
        other.height += 1;
        assert_ne!(coinb1_hex(&job), coinb1_hex(&job_header(&other)));
    }

    #[test]
    fn a_solved_share_meets_its_target() {
        // Grind the extranonce2 like a miner until the regtest target is met, then check the
        // rebuilt header validates.
        let job = job_header(&template());
        let target = Target::from_compact(job.bits);
        let en1 = [9, 9, 9, 9];
        let mut found = None;
        for i in 0u64..4096 {
            let sub = Submission {
                extranonce2: i.to_le_bytes(),
                ntime: [0; 8],
                nonce: [0; 8],
            };
            let h = apply_submission(&job, &en1, &sub);
            if h.meets(target) {
                found = Some(h);
                break;
            }
        }
        let h = found.expect("regtest target is easy");
        assert!(h.validate_pow(target).is_ok());
    }

    #[test]
    fn malformed_submit_fields_are_refused() {
        let ok = "0011223344556677";
        assert!(Submission::parse(None, Some(ok), Some(ok)).is_err());
        assert!(Submission::parse(Some("00112233"), Some(ok), Some(ok)).is_err());
        assert!(Submission::parse(Some(ok), Some("zz11223344556677"), Some(ok)).is_err());
        assert!(Submission::parse(Some(ok), Some(ok), Some("001122334455667788")).is_err());
        assert!(Submission::parse(Some(ok), Some(ok), Some(ok)).is_ok());
    }
}
