use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use anyhow::{bail, Context, Result};
use js_sys::{Array, Function, Object, Promise, Reflect};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use super::{
    append_delete_record, append_put_record, append_transaction_record, parse_log, Entry, BEGIN,
    COMMIT, FILE_HEADER, RECORD_HEADER_LEN,
};
use crate::{Hash, ObjectStore};

const MAX_SAFE_INTEGER: u64 = (1u64 << 53) - 1;

#[wasm_bindgen::prelude::wasm_bindgen(inline_js = r#"
export function chunklogOpfsRead(handle, at, destination) {
    return handle.read(destination, { at });
}

export function chunklogOpfsWrite(handle, at, source) {
    return handle.write(source, { at });
}

export function chunklogOpfsGetSize(handle) {
    return handle.getSize();
}

export function chunklogOpfsFlush(handle) {
    handle.flush();
}

export function chunklogOpfsTruncate(handle, length) {
    handle.truncate(length);
}

export function chunklogOpfsClose(handle) {
    handle.close();
}
"#)]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(catch, js_name = chunklogOpfsRead)]
    fn opfs_read(
        handle: &JsValue,
        at: f64,
        destination: &mut [u8],
    ) -> std::result::Result<f64, JsValue>;

    #[wasm_bindgen::prelude::wasm_bindgen(catch, js_name = chunklogOpfsWrite)]
    fn opfs_write(handle: &JsValue, at: f64, source: &[u8]) -> std::result::Result<f64, JsValue>;

    #[wasm_bindgen::prelude::wasm_bindgen(catch, js_name = chunklogOpfsGetSize)]
    fn opfs_get_size(handle: &JsValue) -> std::result::Result<f64, JsValue>;

    #[wasm_bindgen::prelude::wasm_bindgen(catch, js_name = chunklogOpfsFlush)]
    fn opfs_flush(handle: &JsValue) -> std::result::Result<(), JsValue>;

    #[wasm_bindgen::prelude::wasm_bindgen(catch, js_name = chunklogOpfsTruncate)]
    fn opfs_truncate(handle: &JsValue, len: f64) -> std::result::Result<(), JsValue>;

    #[wasm_bindgen::prelude::wasm_bindgen(catch, js_name = chunklogOpfsClose)]
    fn opfs_close(handle: &JsValue) -> std::result::Result<(), JsValue>;
}

fn js_error(value: JsValue) -> anyhow::Error {
    anyhow::anyhow!("browser API error: {value:?}")
}

fn method(target: &JsValue, name: &str) -> Result<Function> {
    Reflect::get(target, &JsValue::from_str(name))
        .map_err(js_error)?
        .dyn_into::<Function>()
        .map_err(|_| anyhow::anyhow!("browser object has no {name}() method"))
}

fn call(target: &JsValue, name: &str, arguments: &[JsValue]) -> Result<JsValue> {
    let function = method(target, name)?;
    let args = Array::new();
    for argument in arguments {
        args.push(argument);
    }
    function.apply(target, &args).map_err(js_error)
}

async fn await_call(target: &JsValue, name: &str, arguments: &[JsValue]) -> Result<JsValue> {
    let promise = call(target, name, arguments)?
        .dyn_into::<Promise>()
        .map_err(|_| anyhow::anyhow!("{name}() did not return a Promise"))?;
    JsFuture::from(promise).await.map_err(js_error)
}

fn handle_size(handle: &JsValue) -> Result<u64> {
    let value = opfs_get_size(handle).map_err(js_error)?;
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value > MAX_SAFE_INTEGER as f64
    {
        bail!("invalid OPFS file size {value}");
    }
    Ok(value as u64)
}

fn read_exact(handle: &JsValue, offset: u64, destination: &mut [u8]) -> Result<()> {
    let mut completed = 0usize;
    while completed < destination.len() {
        let remaining = destination.len() - completed;
        let chunk = remaining.min(u32::MAX as usize);
        let at = offset
            .checked_add(completed as u64)
            .context("OPFS read offset overflow")?;
        if at > MAX_SAFE_INTEGER {
            bail!("OPFS offset exceeds JavaScript's exact integer range");
        }
        let read = opfs_read(
            handle,
            at as f64,
            &mut destination[completed..completed + chunk],
        )
        .map_err(js_error)? as usize;
        if read == 0 || read > chunk {
            bail!("short OPFS read");
        }
        completed += read;
    }
    Ok(())
}

fn write_all(handle: &JsValue, offset: u64, source: &[u8]) -> Result<()> {
    let mut completed = 0usize;
    while completed < source.len() {
        let chunk = (source.len() - completed).min(u32::MAX as usize);
        let at = offset
            .checked_add(completed as u64)
            .context("OPFS write offset overflow")?;
        if at > MAX_SAFE_INTEGER {
            bail!("OPFS offset exceeds JavaScript's exact integer range");
        }
        let written = opfs_write(handle, at as f64, &source[completed..completed + chunk])
            .map_err(js_error)? as usize;
        if written == 0 || written > chunk {
            bail!("short OPFS write");
        }
        completed += written;
    }
    Ok(())
}

fn truncate(handle: &JsValue, len: u64) -> Result<()> {
    if len > MAX_SAFE_INTEGER {
        bail!("OPFS length exceeds JavaScript's exact integer range");
    }
    opfs_truncate(handle, len as f64).map_err(js_error)
}

fn flush(handle: &JsValue) -> Result<()> {
    opfs_flush(handle).map_err(js_error)
}

struct ActiveBatch {
    id: u64,
    start: u64,
    changes: HashMap<Hash, PendingChange>,
    // One arena avoids an allocation for every object in a large import.
    payloads: Vec<u8>,
}

#[derive(Clone, Copy)]
enum PendingChange {
    Put { offset: usize, len: usize },
    Delete,
}

struct Inner {
    handle: JsValue,
    log: Vec<u8>,
    entries: HashMap<Hash, Entry>,
    len: u64,
    next_transaction: u64,
    active: Option<ActiveBatch>,
}

impl Inner {
    fn begin(&mut self) -> Result<()> {
        if self.active.is_some() {
            bail!("OPFS object batch is already active");
        }
        let id = self.next_transaction;
        self.next_transaction = self.next_transaction.saturating_add(1);
        self.active = Some(ActiveBatch {
            id,
            start: self.len,
            changes: HashMap::new(),
            payloads: Vec::new(),
        });
        Ok(())
    }

    fn commit(&mut self) -> Result<()> {
        let (start, transaction_len) = {
            let batch = self
                .active
                .as_ref()
                .context("no OPFS object batch is active")?;
            if batch.changes.is_empty() {
                self.active = None;
                return Ok(());
            }
            let payload_bytes: usize = batch
                .changes
                .values()
                .filter_map(|change| match change {
                    PendingChange::Put { len, .. } => Some(*len),
                    PendingChange::Delete => None,
                })
                .sum();
            let framing_bytes = batch
                .changes
                .len()
                .checked_mul(RECORD_HEADER_LEN + 32)
                .and_then(|bytes| bytes.checked_add(2 * (RECORD_HEADER_LEN + 8)))
                .context("OPFS transaction framing length overflow")?;
            (
                batch.start,
                payload_bytes
                    .checked_add(framing_bytes)
                    .context("OPFS transaction length overflow")?,
            )
        };

        let start_usize = usize::try_from(start).context("OPFS object log is too large")?;
        if self.log.len() != start_usize || self.len != start {
            bail!("OPFS cached-log length invariant violated");
        }
        let end = start
            .checked_add(transaction_len as u64)
            .context("OPFS object log length overflow")?;
        if end > MAX_SAFE_INTEGER {
            bail!("OPFS object log exceeds JavaScript's exact offset range");
        }

        let effects = {
            let batch = self
                .active
                .as_ref()
                .context("no OPFS object batch is active")?;
            self.log.reserve(transaction_len);
            append_transaction_record(&mut self.log, BEGIN, batch.id);
            let mut effects = Vec::with_capacity(batch.changes.len());
            for (hash, change) in &batch.changes {
                match change {
                    PendingChange::Put { offset, len } => {
                        let end = offset
                            .checked_add(*len)
                            .context("OPFS pending payload range overflow")?;
                        let data = batch
                            .payloads
                            .get(*offset..end)
                            .context("OPFS pending payload range is invalid")?;
                        let record_start = self.log.len();
                        append_put_record(&mut self.log, *hash, data);
                        effects.push((
                            *hash,
                            Some(Entry {
                                offset: record_start as u64 + RECORD_HEADER_LEN as u64 + 32,
                                len: *len as u64,
                            }),
                        ));
                    }
                    PendingChange::Delete => {
                        append_delete_record(&mut self.log, *hash);
                        effects.push((*hash, None));
                    }
                }
            }
            append_transaction_record(&mut self.log, COMMIT, batch.id);
            effects
        };
        if self.log.len() as u64 != end {
            self.log.truncate(start_usize);
            bail!("OPFS serialized transaction length invariant violated");
        }
        if let Err(error) = write_all(&self.handle, start, &self.log[start_usize..])
            .and_then(|()| flush(&self.handle))
        {
            let cleanup = truncate(&self.handle, start).and_then(|()| flush(&self.handle));
            self.log.truncate(start_usize);
            self.len = start;
            return match cleanup {
                Ok(()) => Err(error),
                Err(cleanup_error) => Err(error.context(format!(
                    "also failed to truncate incomplete OPFS transaction: {cleanup_error:#}"
                ))),
            };
        }
        self.len = end;
        for (hash, entry) in effects {
            match entry {
                Some(entry) => {
                    self.entries.insert(hash, entry);
                }
                None => {
                    self.entries.remove(&hash);
                }
            }
        }
        self.active = None;
        Ok(())
    }

    fn rollback(&mut self) -> Result<()> {
        self.active = None;
        Ok(())
    }

    fn entry_bytes(&self, entry: Entry) -> Result<&[u8]> {
        let start = usize::try_from(entry.offset).context("OPFS object offset is too large")?;
        let len = usize::try_from(entry.len).context("OPFS object is too large")?;
        let end = start
            .checked_add(len)
            .context("OPFS object range overflow")?;
        self.log
            .get(start..end)
            .context("OPFS object range is outside the cached log")
    }

    fn read_entry(&self, entry: Entry) -> Result<Vec<u8>> {
        // `parse_log` verifies every recovered PUT before publishing its entry,
        // and new PUTs are digested before their bytes enter this append-only
        // cache. The exclusive access handle prevents a concurrent file writer,
        // so rehashing the same immutable cache range on every read is redundant.
        Ok(self.entry_bytes(entry)?.to_vec())
    }

    fn put(&mut self, data: &[u8]) -> Result<Hash> {
        let hash = Hash::digest(data);
        let pending = self
            .active
            .as_ref()
            .and_then(|batch| batch.changes.get(&hash))
            .copied();
        match pending {
            Some(PendingChange::Put { offset, len }) => {
                let batch = self.active.as_ref().expect("active batch checked above");
                let end = offset
                    .checked_add(len)
                    .context("OPFS pending payload range overflow")?;
                let existing = batch
                    .payloads
                    .get(offset..end)
                    .context("OPFS pending payload range is invalid")?;
                if existing != data {
                    bail!("hash collision while writing pending OPFS object {hash}");
                }
                return Ok(hash);
            }
            Some(PendingChange::Delete) => {
                if let Some(entry) = self.entries.get(&hash).copied() {
                    let existing = self.entry_bytes(entry)?;
                    if existing != data {
                        bail!("hash collision while restoring OPFS object {hash}");
                    }
                    self.active
                        .as_mut()
                        .expect("active batch checked above")
                        .changes
                        .remove(&hash);
                    return Ok(hash);
                }
            }
            None => {}
        }
        if let Some(entry) = self.entries.get(&hash).copied() {
            let existing = self.entry_bytes(entry)?;
            if existing != data {
                bail!("hash collision while writing OPFS object {hash}");
            }
            return Ok(hash);
        }
        if self.active.is_none() {
            bail!("OPFS put requires an active object batch");
        }
        let batch = self.active.as_mut().expect("active batch checked above");
        let offset = batch.payloads.len();
        batch.payloads.extend_from_slice(data);
        batch.changes.insert(
            hash,
            PendingChange::Put {
                offset,
                len: data.len(),
            },
        );
        Ok(hash)
    }

    fn remove(&mut self, hash: Hash) -> Result<()> {
        let batch = self
            .active
            .as_mut()
            .context("OPFS delete requires an active object batch")?;
        match batch.changes.get(&hash) {
            Some(PendingChange::Put { .. }) => {
                batch.changes.remove(&hash);
            }
            Some(PendingChange::Delete) => {}
            None if self.entries.contains_key(&hash) => {
                batch.changes.insert(hash, PendingChange::Delete);
            }
            None => {}
        }
        Ok(())
    }

    fn read(&self, hash: Hash) -> Result<Vec<u8>> {
        if let Some(change) = self
            .active
            .as_ref()
            .and_then(|batch| batch.changes.get(&hash))
        {
            return match change {
                PendingChange::Put { offset, len } => {
                    let end = offset
                        .checked_add(*len)
                        .context("OPFS pending payload range overflow")?;
                    Ok(self
                        .active
                        .as_ref()
                        .expect("active batch checked above")
                        .payloads
                        .get(*offset..end)
                        .context("OPFS pending payload range is invalid")?
                        .to_vec())
                }
                PendingChange::Delete => bail!("object {hash} not found"),
            };
        }
        let entry = self
            .entries
            .get(&hash)
            .copied()
            .with_context(|| format!("object {hash} not found"))?;
        self.read_entry(entry)
    }

    fn list(&self) -> Vec<Hash> {
        let mut hashes: HashSet<_> = self.entries.keys().copied().collect();
        if let Some(batch) = &self.active {
            for (hash, change) in &batch.changes {
                match change {
                    PendingChange::Put { .. } => {
                        hashes.insert(*hash);
                    }
                    PendingChange::Delete => {
                        hashes.remove(hash);
                    }
                }
            }
        }
        hashes.into_iter().collect()
    }
}

/// A verified append-log object store backed by the browser's OPFS.
///
/// Construct and use this store in a dedicated worker. Logical deletion is
/// immediate, but the append-only format does not yet compact dead records.
/// Explicit batches are coalesced into one contiguous write and one flush.
/// The complete log remains cached in wasm memory. Recovered objects are
/// verified before indexing, and synchronous file I/O receives direct views of
/// wasm memory instead of allocating a second JavaScript buffer.
pub struct OpfsStore {
    inner: RefCell<Inner>,
}

impl OpfsStore {
    /// Opens or creates `file_name` in the current origin's OPFS root.
    ///
    /// This rejects on the browser main thread because synchronous access
    /// handles are restricted to dedicated workers.
    pub async fn open(file_name: &str) -> Result<Self> {
        validate_file_name(file_name)?;
        let global = js_sys::global();
        let navigator = Reflect::get(&global, &JsValue::from_str("navigator")).map_err(js_error)?;
        let storage = Reflect::get(&navigator, &JsValue::from_str("storage")).map_err(js_error)?;
        let root = await_call(&storage, "getDirectory", &[]).await?;
        let options = Object::new();
        Reflect::set(
            &options,
            &JsValue::from_str("create"),
            &JsValue::from_bool(true),
        )
        .map_err(js_error)?;
        let file = await_call(
            &root,
            "getFileHandle",
            &[JsValue::from_str(file_name), options.into()],
        )
        .await?;
        let handle = await_call(&file, "createSyncAccessHandle", &[]).await?;
        Self::from_sync_access_handle(handle)
    }

    /// Builds a store from an already acquired `FileSystemSyncAccessHandle`.
    pub fn from_sync_access_handle(handle: JsValue) -> Result<Self> {
        let mut len = handle_size(&handle)?;
        let mut log = if len == 0 {
            write_all(&handle, 0, FILE_HEADER)?;
            flush(&handle)?;
            len = FILE_HEADER.len() as u64;
            FILE_HEADER.to_vec()
        } else {
            let size = usize::try_from(len).context("OPFS log is too large for wasm memory")?;
            let mut bytes = vec![0u8; size];
            read_exact(&handle, 0, &mut bytes)?;
            bytes
        };
        let parsed = parse_log(&log)?;
        if parsed.valid_len != len {
            truncate(&handle, parsed.valid_len)?;
            flush(&handle)?;
            len = parsed.valid_len;
            log.truncate(usize::try_from(len).context("OPFS log is too large for wasm memory")?);
        }
        Ok(Self {
            inner: RefCell::new(Inner {
                handle,
                log,
                entries: parsed.entries,
                len,
                next_transaction: parsed.next_transaction,
                active: None,
            }),
        })
    }
}

fn validate_file_name(name: &str) -> Result<()> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
        bail!("OPFS object-log name must be one non-empty file component");
    }
    Ok(())
}

impl ObjectStore for OpfsStore {
    fn read(&self, hash: Hash) -> Result<Vec<u8>> {
        self.inner.borrow().read(hash)
    }

    fn write(&self, data: &[u8]) -> Result<Hash> {
        let mut inner = self.inner.borrow_mut();
        let automatic = inner.active.is_none();
        if automatic {
            inner.begin()?;
        }
        let result = inner.put(data);
        match result {
            Ok(hash) if automatic => match inner.commit() {
                Ok(()) => Ok(hash),
                Err(error) => {
                    let _ = inner.rollback();
                    Err(error)
                }
            },
            Ok(hash) => Ok(hash),
            Err(error) => {
                if automatic {
                    let _ = inner.rollback();
                }
                Err(error)
            }
        }
    }

    fn list(&self) -> Result<Vec<Hash>> {
        Ok(self.inner.borrow().list())
    }

    fn delete(&self, hash: Hash) -> Result<()> {
        let mut inner = self.inner.borrow_mut();
        let automatic = inner.active.is_none();
        if automatic {
            inner.begin()?;
        }
        let result = inner.remove(hash);
        match result {
            Ok(()) if automatic => match inner.commit() {
                Ok(()) => Ok(()),
                Err(error) => {
                    let _ = inner.rollback();
                    Err(error)
                }
            },
            Ok(()) => Ok(()),
            Err(error) => {
                if automatic {
                    let _ = inner.rollback();
                }
                Err(error)
            }
        }
    }

    fn begin_batch(&self) -> Result<()> {
        self.inner.borrow_mut().begin()
    }

    fn commit_batch(&self) -> Result<()> {
        self.inner.borrow_mut().commit()
    }

    fn rollback_batch(&self) -> Result<()> {
        self.inner.borrow_mut().rollback()
    }
}

impl Drop for OpfsStore {
    fn drop(&mut self) {
        let inner = self.inner.get_mut();
        inner.active = None;
        // Every successful mutation and recovery truncation is flushed before
        // it becomes visible. Closing therefore needs no second durable flush.
        let _ = opfs_close(&inner.handle);
    }
}
