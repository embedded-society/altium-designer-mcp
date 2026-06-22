//! `PcbLib` reader: embedded 3D-model stream parsing + zlib decompression.

#[allow(clippy::wildcard_imports)] // tightly-coupled reader split
use super::*;
use crate::altium::pcblib::primitives::EmbeddedModel;
use flate2::read::ZlibDecoder;
use std::io::Read as IoRead;

/// Parses the `/Library/Models/Data` stream to extract GUID-to-index mapping.
///
/// # Format
///
/// The Data stream contains a sequence of length-prefixed records:
/// ```text
/// [record_len:4 LE][pipe-delimited params][null:1]
/// [record_len:4 LE][pipe-delimited params][null:1]
/// ...
/// ```
///
/// Each record contains pipe-delimited key=value pairs including:
/// - `ID={GUID}` - The model's unique identifier
/// - `NAME=filename.step` - The model filename
/// - `EMBED=TRUE|FALSE` - Whether the model is embedded
/// - `CHECKSUM=...` - Model checksum
///
/// The record's position (0, 1, 2, ...) corresponds to the model stream index
/// (`/Library/Models/0`, `/Library/Models/1`, etc.).
///
/// # Returns
///
/// A `HashMap` mapping GUID strings to their stream index and filename.
pub fn parse_model_data_stream(data: &[u8]) -> ModelIndex {
    let mut index = ModelIndex::new();
    let mut offset = 0usize;
    let mut stream_index = 0usize;

    while offset + 4 <= data.len() {
        // Read 4-byte little-endian record length
        let record_len = u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]) as usize;
        offset += 4;

        if record_len == 0 || offset + record_len > data.len() {
            tracing::debug!(
                offset,
                record_len,
                data_len = data.len(),
                "Invalid record length in Models/Data stream"
            );
            break;
        }

        // Parse the record content as UTF-8 (or Latin-1 fallback)
        let record_data = &data[offset..offset + record_len];
        let record_text = String::from_utf8(record_data.to_vec())
            .unwrap_or_else(|_| record_data.iter().map(|&b| b as char).collect());

        // Extract ID (GUID) and NAME from the record
        let params = crate::altium::parse_pipe_params_raw(&record_text);
        let guid = params.get("ID").cloned().unwrap_or_default();
        let name = params.get("NAME").cloned().unwrap_or_default();

        if !guid.is_empty() {
            tracing::trace!(
                stream_index,
                guid = %guid,
                name = %name,
                "Parsed model record from Data stream"
            );
            index.insert(guid, (stream_index, name));
        }

        // Move past record content and null terminator
        offset += record_len;
        if offset < data.len() && data[offset] == 0 {
            offset += 1;
        }

        stream_index += 1;
    }

    tracing::debug!(count = index.len(), "Parsed model index from Data stream");
    index
}

/// Parses the `/Library/Models/Header` stream to get the model count.
///
/// # Format
///
/// The Header stream is a 4-byte little-endian unsigned integer containing
/// the number of embedded models in the library.
///
/// # Returns
///
/// The number of models in the library, or 0 if parsing fails.
pub fn parse_model_header_stream(data: &[u8]) -> usize {
    if data.len() < 4 {
        tracing::debug!(
            len = data.len(),
            "Models/Header stream too short (expected 4 bytes)"
        );
        return 0;
    }

    let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    tracing::debug!(count, "Parsed model count from Header stream");
    count
}

/// Maximum size we will decompress a single embedded model to.
///
/// This caps decompression bombs: a small zlib stream cannot expand without
/// bound and exhaust memory. The ceiling is deliberately generous — real
/// STEP/IGES models are at most a few megabytes — so legitimate models always
/// fit while a crafted high-ratio stream is rejected.
pub const MAX_DECOMPRESSED_MODEL_BYTES: usize = 256 * 1024 * 1024; // 256 MiB

/// Decompresses a zlib-compressed model stream.
///
/// Models in `/Library/Models/{N}` streams are zlib-compressed STEP files.
///
/// # Arguments
///
/// * `data` - The compressed model data
///
/// # Returns
///
/// The decompressed STEP file data, or an empty vector on error or if the
/// decompressed size exceeds [`MAX_DECOMPRESSED_MODEL_BYTES`].
pub fn decompress_model_data(data: &[u8]) -> Vec<u8> {
    decompress_capped(data, MAX_DECOMPRESSED_MODEL_BYTES)
}

/// Decompresses `data`, rejecting output larger than `max_bytes`.
///
/// The reader is bounded to `max_bytes + 1` so a decompression bomb can never
/// allocate more than that, regardless of the compressed input's expansion
/// ratio. If the limit is reached the stream is treated as hostile/corrupt and
/// an empty vector is returned (the model is then skipped by the caller).
pub(super) fn decompress_capped(data: &[u8], max_bytes: usize) -> Vec<u8> {
    if data.is_empty() {
        return Vec::new();
    }

    // `take` bounds how much we will ever read (and therefore allocate); the
    // `+ 1` lets us detect that the real output exceeded the cap.
    let limit = max_bytes.saturating_add(1) as u64;
    let mut decoder = ZlibDecoder::new(data).take(limit);
    let mut decompressed = Vec::new();

    match decoder.read_to_end(&mut decompressed) {
        Ok(_) => {
            if decompressed.len() > max_bytes {
                tracing::warn!(
                    compressed = data.len(),
                    limit = max_bytes,
                    "Embedded model exceeds the maximum decompressed size; rejecting (possible decompression bomb)"
                );
                return Vec::new();
            }
            tracing::trace!(
                compressed = data.len(),
                decompressed = decompressed.len(),
                "Decompressed model data"
            );
            decompressed
        }
        Err(e) => {
            tracing::debug!(error = %e, "Failed to decompress model data");
            Vec::new()
        }
    }
}

/// Parses embedded models from the `/Library/Models/` storage.
///
/// This function reads the Header and Data streams to understand the model
/// structure, then extracts and decompresses each model.
///
/// # Arguments
///
/// * `model_index` - Mapping of GUID to stream index
/// * `model_data` - Vector of (index, `compressed_data`) pairs
///
/// # Returns
///
/// A vector of `EmbeddedModel` structs with decompressed STEP data.
pub fn parse_embedded_models(
    model_index: &ModelIndex,
    model_data: &[(usize, Vec<u8>)],
) -> Vec<EmbeddedModel> {
    let mut models = Vec::new();

    // Create reverse mapping: index -> (GUID, name)
    let index_to_info: HashMap<usize, (&String, &String)> = model_index
        .iter()
        .map(|(guid, (idx, name))| (*idx, (guid, name)))
        .collect();

    for (idx, compressed) in model_data {
        let Some((guid, name)) = index_to_info.get(idx) else {
            tracing::debug!(index = idx, "Model stream has no GUID mapping");
            continue;
        };

        let decompressed = decompress_model_data(compressed);
        if decompressed.is_empty() {
            tracing::warn!(
                guid = %guid,
                name = %name,
                compressed_size = compressed.len(),
                "Failed to decompress embedded 3D model — model will be missing from library"
            );
            continue;
        }

        let model = EmbeddedModel {
            id: (*guid).clone(),
            name: (*name).clone(),
            data: decompressed,
            compressed_size: compressed.len(),
        };

        tracing::debug!(
            guid = %guid,
            name = %name,
            size = model.data.len(),
            "Parsed embedded model"
        );
        models.push(model);
    }

    models
}
