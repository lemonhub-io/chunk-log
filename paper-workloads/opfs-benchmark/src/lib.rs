#![forbid(unsafe_code)]

use chunklog::{ObjectStore, OpfsStore};
use js_sys::{Function, Reflect};
use wasm_bindgen::{prelude::*, JsCast};

fn browser_error(message: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&message.to_string())
}

fn now() -> Result<f64, JsValue> {
    let global = js_sys::global();
    let performance = Reflect::get(&global, &JsValue::from_str("performance"))?;
    let function = Reflect::get(&performance, &JsValue::from_str("now"))?
        .dyn_into::<Function>()
        .map_err(|_| JsValue::from_str("performance.now is not a function"))?;
    function
        .call0(&performance)?
        .as_f64()
        .ok_or_else(|| JsValue::from_str("performance.now did not return a number"))
}

fn payload(index: u32, size: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; size];
    bytes[..4].copy_from_slice(&index.to_be_bytes());
    let mut state = u64::from(index).wrapping_add(0x9e37_79b9_7f4a_7c15);
    for byte in &mut bytes[4..] {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *byte = state as u8;
    }
    bytes
}

/// Runs one isolated OPFS trial and returns its measurements as JSON.
#[wasm_bindgen]
pub async fn benchmark_trial(
    file_name: String,
    object_count: u32,
    payload_size: u32,
    batched: bool,
) -> Result<String, JsValue> {
    if object_count == 0 {
        return Err(JsValue::from_str("object_count must be positive"));
    }
    if payload_size < 4 {
        return Err(JsValue::from_str("payload_size must be at least four"));
    }

    // Dataset construction is deliberately outside all measured intervals.
    let objects: Vec<Vec<u8>> = (0..object_count)
        .map(|index| payload(index, payload_size as usize))
        .collect();

    let started = now()?;
    let store = OpfsStore::open(&file_name).await.map_err(browser_error)?;
    let open_empty_ms = now()? - started;

    let started = now()?;
    if batched {
        store.begin_batch().map_err(browser_error)?;
    }
    let mut hashes = Vec::with_capacity(objects.len());
    for object in &objects {
        hashes.push(store.write(object).map_err(browser_error)?);
    }
    let stage_writes_ms = now()? - started;
    let commit_started = now()?;
    if batched {
        store.commit_batch().map_err(browser_error)?;
    }
    let batch_commit_ms = now()? - commit_started;
    let import_ms = stage_writes_ms + batch_commit_ms;
    drop(store);

    let started = now()?;
    let reopened = OpfsStore::open(&file_name).await.map_err(browser_error)?;
    let reopen_ms = now()? - started;
    let listed = reopened.list().map_err(browser_error)?.len();
    if listed != objects.len() {
        return Err(browser_error(format!(
            "reopened object count mismatch: expected {}, got {listed}",
            objects.len()
        )));
    }

    let started = now()?;
    let mut checksum = 0u64;
    for (hash, expected) in hashes.into_iter().zip(&objects) {
        let actual = reopened.read(hash).map_err(browser_error)?;
        if &actual != expected {
            return Err(JsValue::from_str("reopened object payload mismatch"));
        }
        checksum = checksum.wrapping_add(actual.iter().map(|byte| u64::from(*byte)).sum::<u64>());
    }
    let read_all_ms = now()? - started;
    drop(reopened);

    Ok(format!(
        "{{\"file_name\":\"{file_name}\",\"object_count\":{object_count},\"payload_size\":{payload_size},\"batched\":{batched},\"open_empty_ms\":{open_empty_ms:.6},\"stage_writes_ms\":{stage_writes_ms:.6},\"batch_commit_ms\":{batch_commit_ms:.6},\"import_ms\":{import_ms:.6},\"reopen_ms\":{reopen_ms:.6},\"read_all_ms\":{read_all_ms:.6},\"checksum\":{checksum}}}"
    ))
}

/// Verifies pending visibility, rollback, delete cancellation and persistence.
#[wasm_bindgen]
pub async fn verify_batch_semantics(file_name: String) -> Result<(), JsValue> {
    let data = b"OPFS batch semantics";
    let store = OpfsStore::open(&file_name).await.map_err(browser_error)?;

    store.begin_batch().map_err(browser_error)?;
    let hash = store.write(data).map_err(browser_error)?;
    if store.read(hash).map_err(browser_error)? != data
        || store.list().map_err(browser_error)? != [hash]
    {
        return Err(JsValue::from_str(
            "pending put is not visible inside its batch",
        ));
    }
    store.rollback_batch().map_err(browser_error)?;
    if !store.list().map_err(browser_error)?.is_empty() || store.read(hash).is_ok() {
        return Err(JsValue::from_str("rollback did not discard pending put"));
    }

    store.write(data).map_err(browser_error)?;
    store.begin_batch().map_err(browser_error)?;
    store.delete(hash).map_err(browser_error)?;
    if store.read(hash).is_ok() || !store.list().map_err(browser_error)?.is_empty() {
        return Err(JsValue::from_str("pending delete is visible incorrectly"));
    }
    store.write(data).map_err(browser_error)?;
    store.commit_batch().map_err(browser_error)?;
    drop(store);

    let reopened = OpfsStore::open(&file_name).await.map_err(browser_error)?;
    if reopened.read(hash).map_err(browser_error)? != data {
        return Err(JsValue::from_str("delete cancellation did not persist"));
    }
    reopened.begin_batch().map_err(browser_error)?;
    reopened.delete(hash).map_err(browser_error)?;
    reopened.commit_batch().map_err(browser_error)?;
    drop(reopened);

    let reopened = OpfsStore::open(&file_name).await.map_err(browser_error)?;
    if !reopened.list().map_err(browser_error)?.is_empty() || reopened.read(hash).is_ok() {
        return Err(JsValue::from_str("committed delete did not persist"));
    }
    Ok(())
}
