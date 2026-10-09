//! `PcbLib` reader: embedded 3D-model stream parsing + zlib decompression.

#[allow(clippy::wildcard_imports)] // tightly-coupled reader split
use super::*;
use crate::altium::inflate::Budget;
use crate::altium::pcblib::primitives::EmbeddedModel;

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

/// The records of a `/Library/Models/Data` stream by model GUID, each with
/// its keys in the order Altium wrote them — what a rewrite replays.
#[must_use]
pub fn parse_model_index_records(data: &[u8]) -> HashMap<String, Vec<(String, String)>> {
    let mut records = HashMap::new();
    let mut offset = 0usize;
    while let Some(len_bytes) = data.get(offset..offset + 4) {
        let len =
            u32::from_le_bytes([len_bytes[0], len_bytes[1], len_bytes[2], len_bytes[3]]) as usize;
        offset += 4;
        let Some(record) = data.get(offset..offset + len).filter(|_| len > 0) else {
            break;
        };
        let text = String::from_utf8(record.to_vec())
            .unwrap_or_else(|_| record.iter().map(|&b| char::from(b)).collect());
        let params = crate::altium::parse_pipe_params_ordered(&text);
        if let Some((_, guid)) = params.iter().find(|(k, _)| k == "ID") {
            if !guid.is_empty() {
                records.insert(guid.clone(), params.clone());
            }
        }
        offset += len;
        if data.get(offset) == Some(&0) {
            offset += 1;
        }
    }
    records
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
/// This caps one decompression bomb: a small zlib stream cannot expand without
/// bound. The ceiling is deliberately generous — real STEP/IGES models are at
/// most a few megabytes — so legitimate models always fit while a crafted
/// high-ratio stream is rejected. The models of a library together are held
/// to the read's [`Budget`].
pub const MAX_DECOMPRESSED_MODEL_BYTES: usize = 256 * 1024 * 1024; // 256 MiB

/// Parses embedded models from the `/Library/Models/` storage.
///
/// This function reads the Header and Data streams to understand the model
/// structure, then extracts and decompresses each model, each to at most
/// [`MAX_DECOMPRESSED_MODEL_BYTES`] and all of them within the read's `budget`.
///
/// # Arguments
///
/// * `model_index` - Mapping of GUID to stream index
/// * `model_data` - Vector of (index, `compressed_data`) pairs
/// * `budget` - What the library read may still inflate
///
/// # Returns
///
/// A vector of `EmbeddedModel` structs with decompressed STEP data.
///
/// # Errors
///
/// When the models inflate past what the read's `budget` allows.
pub fn parse_embedded_models(
    model_index: &ModelIndex,
    model_data: &[(usize, Vec<u8>)],
    budget: &mut Budget,
) -> crate::altium::AltiumResult<Vec<EmbeddedModel>> {
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

        let Some(decompressed) = budget
            .inflate(compressed, MAX_DECOMPRESSED_MODEL_BYTES)?
            .filter(|data| !data.is_empty())
        else {
            tracing::warn!(
                guid = %guid,
                name = %name,
                compressed_size = compressed.len(),
                "Failed to decompress embedded 3D model — model will be missing from library"
            );
            continue;
        };

        let model = EmbeddedModel {
            id: (*guid).clone(),
            name: (*name).clone(),
            data: decompressed,
            compressed_size: compressed.len(),
            index_params: Vec::new(),
            compressed: compressed.clone(),
        };

        tracing::debug!(
            guid = %guid,
            name = %name,
            size = model.data.len(),
            "Parsed embedded model"
        );
        models.push(model);
    }

    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::{parse_model_data_stream, parse_model_header_stream, Budget};

    #[test]
    fn a_models_data_stream_stops_at_the_first_unusable_record_length() {
        // A zero length would loop forever and an overlong one would slice out
        // of bounds, so both end the walk with whatever parsed before them.
        with_tracing(|| {
            assert!(parse_model_data_stream(&[0, 0, 0, 0]).is_empty());

            // Claims 64 bytes with two behind it.
            assert!(parse_model_data_stream(&[64, 0, 0, 0, b'x', b'y']).is_empty());

            // Shorter than a single length word.
            assert!(parse_model_data_stream(&[1, 2, 3]).is_empty());
        });
    }

    /// Runs `body` with a TRACE-level subscriber installed on this thread, so
    /// the diagnostics these parsers emit on a damaged file are actually
    /// formatted rather than skipped by the level check.
    fn with_tracing<T>(body: impl FnOnce() -> T) -> T {
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_test_writer()
            .finish();
        tracing::subscriber::with_default(subscriber, body)
    }

    #[test]
    fn a_short_models_header_reports_no_models_rather_than_guessing() {
        // The count is a 4-byte word; anything shorter is unreadable, and
        // inventing a count would drive a scan for streams that do not exist.
        with_tracing(|| {
            for data in [&[][..], &[1][..], &[1, 0, 0][..]] {
                assert_eq!(parse_model_header_stream(data), 0, "{data:?}");
            }
        });

        assert_eq!(parse_model_header_stream(&[2, 0, 0, 0]), 2);
    }

    #[test]
    fn a_model_whose_stream_will_not_inflate_is_skipped_not_faked() {
        // The index names the model, so dropping it silently is the only way
        // the rest of the library still loads — but it must not appear with
        // empty data, which would write a corrupt model back out.
        let mut index = super::ModelIndex::new();
        index.insert("{AAAA}".to_string(), (0, "part.step".to_string()));
        let data = vec![(0usize, vec![0xFF, 0xFE, 0xFD, 0xFC])];

        let models = with_tracing(|| {
            super::parse_embedded_models(&index, &data, &mut Budget::default())
                .expect("within budget")
        });
        assert!(models.is_empty());
    }
}
