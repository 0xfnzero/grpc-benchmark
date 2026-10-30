//! Version-agnostic decoder for the Solana `Entry` wire format.
//!
//! The benchmark only needs entry boundaries and each transaction's first signature, so this
//! parses just enough of the wire format to find them. It accepts every transaction version
//! currently on mainnet — legacy, v0 and v1 (`0x81`, message first and signatures last) — and the
//! block components that Agave 4.x writes between DATA_COMPLETE boundaries: an entry batch, or a
//! block marker (an entry count of zero followed by `u16 version | u8 variant | u16 len | data`).

/// Largest transaction accepted (v1 raises the limit from 1232 to 4096 bytes).
pub const MAX_TRANSACTION_SIZE: usize = 4096;
/// Upper bound on entries in one batch and on transactions in one entry.
pub const MAX_ENTRY_COUNT: u64 = 50_000;

const SIGNATURE_SIZE: usize = 64;
const PUBKEY_SIZE: usize = 32;
const HASH_SIZE: usize = 32;
const V1_PREFIX: u8 = 0x81;
const BLOCK_MARKER_HEADER_SIZE: usize = 5;
const BLOCK_MARKER_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireError {
    /// More bytes are needed; retry once the next shred payload has been appended.
    Incomplete,
    /// The bytes cannot be a valid entry, transaction or block marker.
    Invalid(&'static str),
}

pub type WireResult<T> = std::result::Result<T, WireError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireTransaction {
    pub first_signature: Option<[u8; SIGNATURE_SIZE]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireEntry {
    pub num_hashes: u64,
    pub hash: [u8; HASH_SIZE],
    pub transactions: Vec<WireTransaction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockComponent {
    EntryBatch(Vec<WireEntry>),
    BlockMarker { version: u16, variant: u8 },
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, len: usize) -> WireResult<&'a [u8]> {
        let end = self.offset.checked_add(len).ok_or(WireError::Invalid("length overflow"))?;
        let slice = self.bytes.get(self.offset..end).ok_or(WireError::Incomplete)?;
        self.offset = end;
        Ok(slice)
    }

    fn skip(&mut self, len: usize) -> WireResult<()> {
        self.take(len).map(|_| ())
    }

    fn u8(&mut self) -> WireResult<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> WireResult<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn u32(&mut self) -> WireResult<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn u64(&mut self) -> WireResult<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    /// Solana compact-u16 ("short_vec") length prefix.
    fn short_vec_len(&mut self) -> WireResult<usize> {
        let mut value = 0usize;
        for shift in [0, 7, 14] {
            let byte = self.u8()?;
            value |= usize::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(WireError::Invalid("short_vec length too long"))
    }

    fn signature(&mut self) -> WireResult<[u8; SIGNATURE_SIZE]> {
        Ok(self.take(SIGNATURE_SIZE)?.try_into().unwrap())
    }
}

/// Parses one wire transaction at the start of `bytes`; returns it and its length.
pub fn parse_transaction(bytes: &[u8]) -> WireResult<(WireTransaction, usize)> {
    let mut reader = Reader::new(bytes);
    let first_signature = if bytes.first() == Some(&V1_PREFIX) {
        parse_v1(&mut reader)?
    } else {
        parse_legacy_or_v0(&mut reader)?
    };
    if reader.offset > MAX_TRANSACTION_SIZE {
        return Err(WireError::Invalid("transaction exceeds maximum size"));
    }
    Ok((WireTransaction { first_signature }, reader.offset))
}

/// legacy / v0: `short_vec<signature>` then the message.
fn parse_legacy_or_v0(reader: &mut Reader) -> WireResult<Option<[u8; SIGNATURE_SIZE]>> {
    let num_signatures = reader.short_vec_len()?;
    let first_signature = if num_signatures > 0 { Some(reader.signature()?) } else { None };
    reader.skip(num_signatures.saturating_sub(1) * SIGNATURE_SIZE)?;

    let version_or_header = *reader.bytes.get(reader.offset).ok_or(WireError::Incomplete)?;
    let is_v0 = match version_or_header {
        0x80 => true,
        prefix if prefix & 0x80 != 0 => return Err(WireError::Invalid("unsupported message version")),
        _ => false,
    };
    if is_v0 {
        reader.skip(1)?;
    }
    reader.skip(3)?; // message header
    let num_keys = reader.short_vec_len()?;
    reader.skip(num_keys * PUBKEY_SIZE)?;
    reader.skip(HASH_SIZE)?; // recent blockhash
    let num_instructions = reader.short_vec_len()?;
    for _ in 0..num_instructions {
        reader.skip(1)?; // program id index
        let num_accounts = reader.short_vec_len()?;
        reader.skip(num_accounts)?;
        let data_len = reader.short_vec_len()?;
        reader.skip(data_len)?;
    }
    if is_v0 {
        let num_lookups = reader.short_vec_len()?;
        for _ in 0..num_lookups {
            reader.skip(PUBKEY_SIZE)?;
            let writable = reader.short_vec_len()?;
            reader.skip(writable)?;
            let readonly = reader.short_vec_len()?;
            reader.skip(readonly)?;
        }
    }
    Ok(first_signature)
}

/// v1: `0x81 | header[3] | config mask u32 | lifetime[32] | u8 #instructions | u8 #addresses |
/// addresses | config values (4 bytes per mask bit) | instruction headers (u8 program, u8 #accounts,
/// u16 data len) | instruction payloads | signatures (one per required signer)`.
fn parse_v1(reader: &mut Reader) -> WireResult<Option<[u8; SIGNATURE_SIZE]>> {
    reader.skip(1)?; // version prefix
    let num_signatures = usize::from(reader.u8()?);
    reader.skip(2)?; // readonly signed / unsigned counts
    let config_mask = reader.u32()?;
    reader.skip(HASH_SIZE)?;
    let num_instructions = usize::from(reader.u8()?);
    let num_addresses = usize::from(reader.u8()?);
    reader.skip(num_addresses * PUBKEY_SIZE)?;
    reader.skip(config_mask.count_ones() as usize * 4)?;
    let mut payload_len = 0usize;
    for _ in 0..num_instructions {
        reader.skip(1)?; // program id index
        let num_accounts = usize::from(reader.u8()?);
        let data_len = usize::from(reader.u16()?);
        payload_len += num_accounts + data_len;
    }
    reader.skip(payload_len)?;
    let first_signature = if num_signatures > 0 { Some(reader.signature()?) } else { None };
    reader.skip(num_signatures.saturating_sub(1) * SIGNATURE_SIZE)?;
    Ok(first_signature)
}

/// Parses one `Entry` (`num_hashes u64 | hash | u64 #transactions | transactions`).
pub fn parse_entry(bytes: &[u8]) -> WireResult<(WireEntry, usize)> {
    let mut reader = Reader::new(bytes);
    let num_hashes = reader.u64()?;
    let hash = reader.take(HASH_SIZE)?.try_into().unwrap();
    let num_transactions = reader.u64()?;
    if num_transactions > MAX_ENTRY_COUNT {
        return Err(WireError::Invalid("transaction count exceeds limit"));
    }
    let mut transactions = Vec::with_capacity(num_transactions.min(64) as usize);
    for _ in 0..num_transactions {
        let (transaction, len) = parse_transaction(&bytes[reader.offset..])?;
        reader.offset += len;
        transactions.push(transaction);
    }
    Ok((WireEntry { num_hashes, hash, transactions }, reader.offset))
}

/// Reads the u64 entry count that starts every entry batch and block marker.
pub fn parse_entry_count(bytes: &[u8]) -> WireResult<u64> {
    let count = Reader::new(bytes).u64()?;
    if count > MAX_ENTRY_COUNT {
        return Err(WireError::Invalid("entry count exceeds limit"));
    }
    Ok(count)
}

/// Parses a `Vec<Entry>` (u64 count, then entries); returns the entries and bytes consumed.
pub fn parse_entries(bytes: &[u8]) -> WireResult<(Vec<WireEntry>, usize)> {
    let count = parse_entry_count(bytes)?;
    let mut offset = 8;
    let mut entries = Vec::with_capacity(count.min(1024) as usize);
    for _ in 0..count {
        let (entry, len) = parse_entry(&bytes[offset..])?;
        offset += len;
        entries.push(entry);
    }
    Ok((entries, offset))
}

/// Length of a block marker body (after its zero entry count), and its version and variant.
pub fn parse_block_marker_body(bytes: &[u8]) -> WireResult<(u16, u8, usize)> {
    let mut reader = Reader::new(bytes);
    let version = reader.u16()?;
    if version != BLOCK_MARKER_VERSION {
        return Err(WireError::Invalid("unknown block marker version"));
    }
    let variant = reader.u8()?;
    let len = usize::from(reader.u16()?);
    reader.skip(len)?;
    debug_assert_eq!(reader.offset, BLOCK_MARKER_HEADER_SIZE + len);
    Ok((version, variant, reader.offset))
}

/// Parses one block component: an entry batch, or a block marker when the entry count is zero.
pub fn parse_block_component(bytes: &[u8]) -> WireResult<(BlockComponent, usize)> {
    if parse_entry_count(bytes)? == 0 {
        let (version, variant, len) = parse_block_marker_body(&bytes[8..])?;
        return Ok((BlockComponent::BlockMarker { version, variant }, 8 + len));
    }
    let (entries, len) = parse_entries(bytes)?;
    Ok((BlockComponent::EntryBatch(entries), len))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Real mainnet transactions (slot 462331012), byte-for-byte as they appear inside entries.
    const LEGACY_TX_HEX: &str = include_str!("../tests/fixtures/legacy_tx.hex");
    const V0_TX_HEX: &str = include_str!("../tests/fixtures/v0_tx.hex");
    const V1_TX_HEX: &str = include_str!("../tests/fixtures/v1_tx.hex");

    fn hex(s: &str) -> Vec<u8> {
        let s = s.trim();
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    #[test]
    fn parses_every_transaction_version_to_its_exact_length() {
        for (name, tx) in [("legacy", LEGACY_TX_HEX), ("v0", V0_TX_HEX), ("v1", V1_TX_HEX)] {
            let tx = hex(tx);
            let (parsed, len) = parse_transaction(&tx).unwrap();
            assert_eq!(len, tx.len(), "{name}");
            let expected = if tx[0] == V1_PREFIX {
                let n = usize::from(tx[1]);
                &tx[tx.len() - 64 * n..tx.len() - 64 * (n - 1)]
            } else {
                &tx[1..65]
            };
            assert_eq!(parsed.first_signature.unwrap().as_slice(), expected, "{name}");
            for cut in [1, tx.len() / 2, tx.len() - 1] {
                assert_eq!(parse_transaction(&tx[..cut]).unwrap_err(), WireError::Incomplete, "{name} cut {cut}");
            }
        }
    }

    #[test]
    fn parses_entry_batch_and_block_markers() {
        let txs: Vec<Vec<u8>> = [LEGACY_TX_HEX, V0_TX_HEX, V1_TX_HEX].iter().map(|t| hex(t)).collect();
        let mut batch = 2u64.to_le_bytes().to_vec();
        for chunk in [&txs[..2], &txs[2..]] {
            batch.extend(1u64.to_le_bytes());
            batch.extend([7u8; 32]);
            batch.extend((chunk.len() as u64).to_le_bytes());
            chunk.iter().for_each(|t| batch.extend(t));
        }
        let (component, len) = parse_block_component(&batch).unwrap();
        assert_eq!(len, batch.len());
        let BlockComponent::EntryBatch(entries) = component else { panic!("expected entries") };
        assert_eq!(entries.iter().map(|e| e.transactions.len()).sum::<usize>(), 3);
        // trailing zero padding after a batch is left to the caller
        let mut padded = batch.clone();
        padded.extend([0u8; 100]);
        assert_eq!(parse_block_component(&padded).unwrap().1, batch.len());

        // block header marker: zero count | version 1 | variant 1 | len 41 | data
        let mut marker = 0u64.to_le_bytes().to_vec();
        marker.extend(1u16.to_le_bytes());
        marker.push(1);
        marker.extend(41u16.to_le_bytes());
        marker.extend([1u8; 41]);
        assert_eq!(
            parse_block_component(&marker).unwrap(),
            (BlockComponent::BlockMarker { version: 1, variant: 1 }, marker.len())
        );
        assert_eq!(parse_block_component(&marker[..marker.len() - 1]).unwrap_err(), WireError::Incomplete);
        assert!(matches!(parse_entry_count(&50_001u64.to_le_bytes()), Err(WireError::Invalid(_))));
    }
}
