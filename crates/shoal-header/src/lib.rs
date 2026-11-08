// SPDX-License-Identifier: MIT OR Apache-2.0
//! XBT (Bitcoin on BLAKE2b, Knots-BLAKE2b) block header v2, shaped like rust-bitcoin's
//! `block::Header` so that sharechain code written against rust-bitcoin can use it with
//! few changes.
//!
//! The six legacy fields keep rust-bitcoin's names and types (`version`, `prev_blockhash`,
//! `merkle_root`, `time`, `bits`, `nonce`), so field reads compile unchanged. The v2-only
//! fields sit in [`V2Fields`].
//!
//! Hashing and byte layout come from `lazarus-protocol` (`pow.rs`), which is checked against
//! mainnet blocks. Byte order:
//! - the header stores `prev_blockhash` and `merkle_root` in rust-bitcoin's internal order,
//!   exactly as Bitcoin does;
//! - XBT's block id is the BLAKE2b PoW hash read most-significant-byte first. So
//!   [`XbtHeader::block_hash`] returns it byte-reversed as a `BlockHash`. Then
//!   `Target::is_met_by`, `Display` and the block ids printed by Knots all agree.
//!
//! Transaction ids and the merkle tree stay SHA-256d on XBT; only the header PoW differs.

use bitcoin::block::Version;
use bitcoin::consensus::encode::{self, Decodable, Encodable};
use bitcoin::hashes::Hash;
use bitcoin::io::{Read, Write};
use bitcoin::pow::{CompactTarget, Target};
use bitcoin::{BlockHash, Network, TxMerkleNode};
use lazarus_protocol::pow::{HeaderV2, HEADER_V2_SIZE, V2_FLAG};
use serde::{Deserialize, Serialize};

pub use lazarus_protocol::pow::HEADER_V2_SIZE as XBT_HEADER_SIZE;

/// The header fields that only exist in XBT's v2 header (bytes 80..164).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct V2Fields {
    pub nonce2: u32,
    pub nonce3: u32,
    /// The Stratum V1 extranonce. It lives in the header, not the coinbase, so the ASIC never
    /// sees the coinbase.
    pub extranonce: [u8; 16],
    pub time_offset: u32,
    pub txcount: u16,
    pub flags: u8,
    pub xor_key_mask_clear_bits: u8,
    pub xor_key: [u8; 16],
    pub height: i32,
    /// Reserved merge-mining hook (bytes 132..164). Consensus-inactive; Shoal leaves it zero.
    pub mm_rhs: [u8; 32],
}

/// An XBT 164-byte v2 block header.
///
/// `time` is the logical block time; when [`V2Fields::flags`] has the time-offset bit, the time
/// on the wire is `time - time_offset` (the miner's 8-byte ntime is nonce space).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct XbtHeader {
    /// Block version, without the v2 flag bit (which encoding always sets).
    pub version: Version,
    pub prev_blockhash: BlockHash,
    pub merkle_root: TxMerkleNode,
    pub time: u32,
    pub bits: CompactTarget,
    pub nonce: u32,
    pub v2: V2Fields,
}

/// Why a header failed [`XbtHeader::validate_pow`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PowError {
    /// `bits` does not encode the target the caller requires.
    BadTarget,
    /// The PoW hash is above the target.
    BadProofOfWork,
}

impl std::fmt::Display for PowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PowError::BadTarget => f.write_str("header bits do not match the required target"),
            PowError::BadProofOfWork => f.write_str("BLAKE2b proof of work is above the target"),
        }
    }
}

impl std::error::Error for PowError {}

impl XbtHeader {
    /// Build from the `lazarus-protocol` header type.
    pub fn from_lazarus(h: &HeaderV2) -> Self {
        XbtHeader {
            version: Version::from_consensus(h.version & !(V2_FLAG as i32)),
            prev_blockhash: BlockHash::from_byte_array(h.prev_block),
            merkle_root: TxMerkleNode::from_byte_array(h.merkle_root),
            time: h.time,
            bits: CompactTarget::from_consensus(h.bits),
            nonce: h.nonce,
            v2: V2Fields {
                nonce2: h.nonce2,
                nonce3: h.nonce3,
                extranonce: h.extranonce,
                time_offset: h.time_offset,
                txcount: h.txcount,
                flags: h.flags,
                xor_key_mask_clear_bits: h.xor_key_mask_clear_bits,
                xor_key: h.xor_key,
                height: h.height,
                mm_rhs: h.mm_rhs,
            },
        }
    }

    /// Convert to the `lazarus-protocol` header type (for hashing and Stratum job building).
    pub fn to_lazarus(&self) -> HeaderV2 {
        HeaderV2 {
            version: self.version.to_consensus() & !(V2_FLAG as i32),
            prev_block: self.prev_blockhash.to_byte_array(),
            merkle_root: self.merkle_root.to_byte_array(),
            time: self.time,
            bits: self.bits.to_consensus(),
            nonce: self.nonce,
            nonce2: self.v2.nonce2,
            nonce3: self.v2.nonce3,
            extranonce: self.v2.extranonce,
            time_offset: self.v2.time_offset,
            txcount: self.v2.txcount,
            flags: self.v2.flags,
            xor_key_mask_clear_bits: self.v2.xor_key_mask_clear_bits,
            xor_key: self.v2.xor_key,
            height: self.v2.height,
            mm_rhs: self.v2.mm_rhs,
        }
    }

    /// The 164 bytes as they go into a block.
    pub fn to_bytes(&self) -> [u8; HEADER_V2_SIZE] {
        self.to_lazarus().serialize()
    }

    /// Parse 164 bytes. Refuses a header without the v2 flag: every XBT block since the BLAKE2b
    /// fork is v2, and accepting v1 bytes would give one header two encodings.
    pub fn from_bytes(bytes: &[u8; HEADER_V2_SIZE]) -> Result<Self, encode::Error> {
        let raw_version = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if raw_version & V2_FLAG == 0 {
            return Err(encode::Error::ParseFailed("not an XBT v2 header (v2 flag clear)"));
        }
        let header = Self::from_lazarus(&HeaderV2::deserialize(bytes));
        // Reject non-canonical encodings: decoding then encoding must give the same bytes.
        if header.to_bytes() != *bytes {
            return Err(encode::Error::ParseFailed("non-canonical XBT v2 header"));
        }
        Ok(header)
    }

    /// The BLAKE2b PoW hash, most significant byte first (the order Knots prints block ids in).
    pub fn pow_hash_be(&self) -> [u8; 32] {
        self.to_lazarus().pow_hash()
    }

    /// The block id, as a rust-bitcoin `BlockHash` (internal, byte-reversed order).
    pub fn block_hash(&self) -> BlockHash {
        let mut le = self.pow_hash_be();
        le.reverse();
        BlockHash::from_byte_array(le)
    }

    /// The target `bits` encodes.
    pub fn target(&self) -> Target {
        Target::from_compact(self.bits)
    }

    /// Difficulty relative to the network's maximum target, as rust-bitcoin computes it.
    /// XBT's maximum target is Bitcoin's, so the same table applies.
    pub fn difficulty(&self, network: Network) -> u128 {
        self.target().difficulty(network)
    }

    /// Check that `bits` encodes `required_target` and that the BLAKE2b PoW meets it.
    /// Returns the block hash on success, like rust-bitcoin's `Header::validate_pow`.
    pub fn validate_pow(&self, required_target: Target) -> Result<BlockHash, PowError> {
        if self.target() != required_target {
            return Err(PowError::BadTarget);
        }
        let hash = self.block_hash();
        if required_target.is_met_by(hash) {
            Ok(hash)
        } else {
            Err(PowError::BadProofOfWork)
        }
    }

    /// True if the PoW meets `target`, whatever `bits` says. Used for share targets, which are
    /// easier than the block target in `bits`.
    pub fn meets(&self, target: Target) -> bool {
        target.is_met_by(self.block_hash())
    }
}

impl Encodable for XbtHeader {
    fn consensus_encode<W: Write + ?Sized>(&self, w: &mut W) -> Result<usize, bitcoin::io::Error> {
        let bytes = self.to_bytes();
        w.write_all(&bytes)?;
        Ok(bytes.len())
    }
}

impl Decodable for XbtHeader {
    fn consensus_decode<R: Read + ?Sized>(r: &mut R) -> Result<Self, encode::Error> {
        let mut bytes = [0u8; HEADER_V2_SIZE];
        r.read_exact(&mut bytes)?;
        Self::from_bytes(&bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::consensus::{deserialize, serialize};

    /// Mainnet headers and their block ids (same vectors as lazarus-protocol's
    /// `mainnet_headers_hash_to_their_block_id`).
    const VECTORS: [(&str, &str, i32); 4] = [
        (
            "000000a08a5b7dbc0337b142a753a5866bd75425b5836645d8b7bbbd4a0000000000000037510619fdaa9e4789db4474492959bead42940083793308eda540d6f28cb38cb710956a4f8d001aff7b6153f35f0386b710956a00000000b10cf00d04000000000000000000000061030000000000000000000000000000000000002eae0e000000000000000000000000000000000000000000000000000000000000000000",
            "00000000000000752ea27e251cdfdfe7add3023d8da73bddb6483832841f5d4d",
            962094,
        ),
        (
            "000000a0feec7d41958832724007c3f9c5f92eeec9571abb4b67f7ef870000000000000025e7be2c7e2a940dfa98ead7149f7bf242afa19cf45c95b74771ddd654be9501760f956a4f8d001ad9a44074e169ae52760f956a00000000b14cf00e1200000000000000000000007c030000000000000000000000000000000000002dae0e000000000000000000000000000000000000000000000000000000000000000000",
            "000000000000004abdbbb7d8456683b52554d76b86a553a742b13703bc7d5b8a",
            962093,
        ),
        (
            "000000a0b6e1385dff49ee60384703630c718db81fa5023fde3a964e8500000000000000c5bb5d4e36e8d78ea11f21637f363dc59dca4e18d9e78360820bdb4e186c00fb3a0f956a4f8d001ac685dfa29acae44a3a0f956a00000000b10cf00c04000000000000000000000095000000000000000000000000000000000000002cae0e000000000000000000000000000000000000000000000000000000000000000000",
            "0000000000000087eff7674bbb1a57c9ee2ef9c5f9c3074072328895417decfe",
            962092,
        ),
        (
            "000000a0e2dc0205107ac0d6dd56db39646d405ab6eb0f10e976a1da83000000000000001e42d9ddd07f36b18b58629c3745d550b268b6ff742cf036e440ef6fd56bf47fb4f4946a4f8d001aed47399af5f6203bb4f4946a00000000b10cf00d020000000000000000000000ea03040000000000000000000000000000000000fdad0e000000000000000000000000000000000000000000000000000000000000000000",
            "000000000000001a35e6b1647d02f7cdfa1e1b67d8bdd4c6ef1c2dbda524b9af",
            962045,
        ),
    ];

    fn header(i: usize) -> (XbtHeader, Vec<u8>) {
        let raw = hex::decode(VECTORS[i].0).unwrap();
        (deserialize::<XbtHeader>(&raw).unwrap(), raw)
    }

    #[test]
    fn mainnet_headers_give_their_block_ids() {
        for (i, (_, id, height)) in VECTORS.iter().enumerate() {
            let (h, _) = header(i);
            assert_eq!(h.block_hash().to_string(), *id, "height {height}");
            assert_eq!(h.v2.height, *height);
        }
    }

    #[test]
    fn mainnet_headers_meet_their_own_target() {
        for i in 0..VECTORS.len() {
            let (h, _) = header(i);
            assert_eq!(h.validate_pow(h.target()), Ok(h.block_hash()));
        }
    }

    #[test]
    fn encoding_round_trips_byte_for_byte() {
        for i in 0..VECTORS.len() {
            let (h, raw) = header(i);
            assert_eq!(serialize(&h), raw);
            assert_eq!(serialize(&h).len(), XBT_HEADER_SIZE);
        }
    }

    #[test]
    fn chain_links_prev_blockhash_to_parent_block_hash() {
        // 962094's parent is 962093, 962093's is 962092: the header's prev field (internal
        // order) must equal the parent's block_hash, as in rust-bitcoin.
        let (h94, _) = header(0);
        let (h93, _) = header(1);
        let (h92, _) = header(2);
        assert_eq!(h94.prev_blockhash, h93.block_hash());
        assert_eq!(h93.prev_blockhash, h92.block_hash());
    }

    #[test]
    fn any_changed_field_breaks_the_proof_of_work() {
        let (h, _) = header(0);
        let mut changed = h;
        changed.nonce = changed.nonce.wrapping_add(1);
        assert_eq!(changed.validate_pow(h.target()), Err(PowError::BadProofOfWork));
        let mut changed = h;
        changed.v2.extranonce[15] ^= 1;
        assert_ne!(changed.block_hash(), h.block_hash());
        let mut changed = h;
        changed.v2.xor_key[0] ^= 1;
        assert_ne!(changed.block_hash(), h.block_hash());
        let mut changed = h;
        changed.v2.mm_rhs[0] = 1;
        assert_ne!(changed.block_hash(), h.block_hash(), "reserved slot is committed by h2");
    }

    #[test]
    fn wrong_required_target_is_refused() {
        let (h, _) = header(0);
        let easier = Target::from_compact(CompactTarget::from_consensus(0x207fffff));
        assert_eq!(h.validate_pow(easier), Err(PowError::BadTarget));
        // ...but the same header does meet the easier target as a share.
        assert!(h.meets(easier));
    }

    #[test]
    fn v1_header_bytes_are_refused() {
        let (_, mut raw) = header(0);
        raw[3] &= 0x7f; // clear the v2 flag
        assert!(deserialize::<XbtHeader>(&raw).is_err());
    }

    #[test]
    fn truncated_header_is_refused() {
        let (_, raw) = header(0);
        assert!(deserialize::<XbtHeader>(&raw[..80]).is_err());
        assert!(deserialize::<XbtHeader>(&raw[..163]).is_err());
    }

    #[test]
    fn version_excludes_the_v2_flag() {
        let (h, _) = header(0);
        assert_eq!(h.version.to_consensus() as u32 & V2_FLAG, 0);
    }
}
