//! Origin Private File System object storage for WebAssembly.
//!
//! The browser backend uses one append-only file with explicit transaction
//! markers. Its synchronous [`ObjectStore`](crate::ObjectStore) operations
//! require a `FileSystemSyncAccessHandle`, so it must run in a dedicated
//! worker. Handle acquisition remains asynchronous.

#![cfg_attr(not(any(test, target_arch = "wasm32")), allow(dead_code))]

use std::collections::HashMap;

use anyhow::{bail, Context, Result};

use crate::Hash;

const FILE_HEADER: &[u8; 8] = b"CHOP\x01\0\0\0";
const RECORD_HEADER_LEN: usize = 9;
const BEGIN: u8 = 1;
const PUT: u8 = 2;
const DELETE: u8 = 3;
const COMMIT: u8 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Entry {
    offset: u64,
    len: u64,
}

#[derive(Debug)]
struct ParsedLog {
    entries: HashMap<Hash, Entry>,
    valid_len: u64,
    next_transaction: u64,
}

enum PendingOperation {
    Put(Hash, Entry),
    Delete(Hash),
}

struct PendingTransaction {
    id: u64,
    start: usize,
    operations: Vec<PendingOperation>,
}

fn append_record(bytes: &mut Vec<u8>, kind: u8, payload: &[u8]) {
    bytes.push(kind);
    bytes.extend_from_slice(&(payload.len() as u64).to_be_bytes());
    bytes.extend_from_slice(payload);
}

#[cfg(test)]
fn transaction_record(kind: u8, id: u64) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(RECORD_HEADER_LEN + 8);
    append_transaction_record(&mut bytes, kind, id);
    bytes
}

fn append_transaction_record(bytes: &mut Vec<u8>, kind: u8, id: u64) {
    append_record(bytes, kind, &id.to_be_bytes());
}

#[cfg(test)]
fn put_record(hash: Hash, data: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(RECORD_HEADER_LEN + 32 + data.len());
    append_put_record(&mut bytes, hash, data);
    bytes
}

fn append_put_record(bytes: &mut Vec<u8>, hash: Hash, data: &[u8]) {
    bytes.push(PUT);
    bytes.extend_from_slice(&((32 + data.len()) as u64).to_be_bytes());
    bytes.extend_from_slice(&hash.0);
    bytes.extend_from_slice(data);
}

#[cfg(test)]
fn delete_record(hash: Hash) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(RECORD_HEADER_LEN + 32);
    append_delete_record(&mut bytes, hash);
    bytes
}

fn append_delete_record(bytes: &mut Vec<u8>, hash: Hash) {
    append_record(bytes, DELETE, &hash.0);
}

fn parse_log(bytes: &[u8]) -> Result<ParsedLog> {
    if bytes.len() < FILE_HEADER.len() || &bytes[..FILE_HEADER.len()] != FILE_HEADER {
        bail!("invalid OPFS object-log header");
    }
    let mut entries = HashMap::new();
    let mut pending: Option<PendingTransaction> = None;
    let mut offset = FILE_HEADER.len();
    let mut valid_len = offset;
    let mut next_transaction = 1u64;

    while offset < bytes.len() {
        let record_start = offset;
        if bytes.len() - offset < RECORD_HEADER_LEN {
            break;
        }
        let kind = bytes[offset];
        let len = u64::from_be_bytes(bytes[offset + 1..offset + 9].try_into().unwrap());
        let len = usize::try_from(len).context("OPFS record is too large for this platform")?;
        offset += RECORD_HEADER_LEN;
        let Some(end) = offset.checked_add(len) else {
            bail!("OPFS record length overflow");
        };
        if end > bytes.len() {
            break;
        }
        let payload = &bytes[offset..end];

        match kind {
            BEGIN => {
                if pending.is_some() || payload.len() != 8 {
                    bail!("invalid nested OPFS transaction");
                }
                let id = u64::from_be_bytes(payload.try_into().unwrap());
                next_transaction = next_transaction.max(id.saturating_add(1));
                pending = Some(PendingTransaction {
                    id,
                    start: record_start,
                    operations: Vec::new(),
                });
            }
            PUT => {
                let transaction = pending
                    .as_mut()
                    .context("OPFS put record appears outside a transaction")?;
                if payload.len() < 32 {
                    bail!("truncated OPFS put record");
                }
                let hash = Hash(payload[..32].try_into().unwrap());
                let data = &payload[32..];
                hash.verify(data).context("corrupt OPFS put record")?;
                transaction.operations.push(PendingOperation::Put(
                    hash,
                    Entry {
                        offset: (offset + 32) as u64,
                        len: data.len() as u64,
                    },
                ));
            }
            DELETE => {
                let transaction = pending
                    .as_mut()
                    .context("OPFS delete record appears outside a transaction")?;
                if payload.len() != 32 {
                    bail!("invalid OPFS delete record");
                }
                transaction
                    .operations
                    .push(PendingOperation::Delete(Hash(payload.try_into().unwrap())));
            }
            COMMIT => {
                if payload.len() != 8 {
                    bail!("invalid OPFS commit record");
                }
                let id = u64::from_be_bytes(payload.try_into().unwrap());
                let transaction = pending
                    .take()
                    .context("OPFS commit appears without a transaction")?;
                if transaction.id != id {
                    bail!("OPFS transaction identifier mismatch");
                }
                for operation in transaction.operations {
                    match operation {
                        PendingOperation::Put(hash, entry) => {
                            entries.insert(hash, entry);
                        }
                        PendingOperation::Delete(hash) => {
                            entries.remove(&hash);
                        }
                    }
                }
                valid_len = end;
            }
            other => bail!("unknown OPFS record kind {other}"),
        }
        offset = end;
    }

    if let Some(transaction) = pending {
        valid_len = transaction.start;
    }
    Ok(ParsedLog {
        entries,
        valid_len: valid_len as u64,
        next_transaction,
    })
}

#[cfg(target_arch = "wasm32")]
mod browser;

#[cfg(target_arch = "wasm32")]
pub use browser::OpfsStore;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_batches_are_indexed_and_incomplete_tail_is_ignored() {
        let data = b"canonical object";
        let hash = Hash::digest(data);
        let mut log = FILE_HEADER.to_vec();
        log.extend(transaction_record(BEGIN, 1));
        log.extend(put_record(hash, data));
        log.extend(transaction_record(COMMIT, 1));
        let committed_len = log.len() as u64;
        log.extend(transaction_record(BEGIN, 2));
        log.extend(delete_record(hash));

        let parsed = parse_log(&log).unwrap();
        assert_eq!(parsed.valid_len, committed_len);
        assert_eq!(parsed.next_transaction, 3);
        assert_eq!(parsed.entries.len(), 1);
        let entry = parsed.entries[&hash];
        assert_eq!(&log[entry.offset as usize..][..entry.len as usize], data);
    }

    #[test]
    fn corruption_and_unframed_operations_are_rejected() {
        assert!(parse_log(b"not an object log").is_err());
        let hash = Hash::digest(b"value");
        let mut unframed = FILE_HEADER.to_vec();
        unframed.extend(put_record(hash, b"value"));
        assert!(parse_log(&unframed).is_err());

        let mut corrupt = FILE_HEADER.to_vec();
        corrupt.extend(transaction_record(BEGIN, 1));
        corrupt.extend(put_record(hash, b"value"));
        *corrupt.last_mut().unwrap() ^= 1;
        assert!(parse_log(&corrupt).is_err());
    }
}
