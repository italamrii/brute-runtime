//! Streaming GGUF metadata reader.
//!
//! Reads only the header, the metadata key/value table, and the tensor-info
//! table - never the tensor data blob itself. Every string and array is
//! sanity-capped against the actual remaining file size so a hostile or
//! corrupt length field is rejected instead of causing a huge allocation.
//!
//! Format reference: <https://github.com/ggml-org/ggml/blob/master/docs/gguf.md>

use crate::errors::GgufError;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

const MAGIC: [u8; 4] = *b"GGUF";
const SUPPORTED_VERSIONS: [u32; 2] = [2, 3];

// Sanity caps. These exist purely to reject hostile/corrupt count fields
// before we act on them (e.g. seeking or looping) - real GGUF files are
// nowhere near these limits.
const MAX_KV_COUNT: u64 = 500_000;
const MAX_TENSOR_COUNT: u64 = 2_000_000;
const MAX_STRING_LEN: u64 = 64 * 1024 * 1024; // 64 MiB
const MAX_ARRAY_LEN: u64 = 200_000_000;
const MAX_DIMS: u32 = 8;

const GGUF_TYPE_U8: u32 = 0;
const GGUF_TYPE_I8: u32 = 1;
const GGUF_TYPE_U16: u32 = 2;
const GGUF_TYPE_I16: u32 = 3;
const GGUF_TYPE_U32: u32 = 4;
const GGUF_TYPE_I32: u32 = 5;
const GGUF_TYPE_F32: u32 = 6;
const GGUF_TYPE_BOOL: u32 = 7;
const GGUF_TYPE_STRING: u32 = 8;
const GGUF_TYPE_ARRAY: u32 = 9;
const GGUF_TYPE_U64: u32 = 10;
const GGUF_TYPE_I64: u32 = 11;
const GGUF_TYPE_F64: u32 = 12;

/// Keys we bother materializing into `kv_preview`. Everything else is
/// parsed (to keep the stream position correct) and discarded.
const PREVIEW_KEYS: &[&str] = &[
    "general.architecture",
    "general.name",
    "general.quantization_version",
    "general.file_type",
    "general.alignment",
    "general.parameter_count",
    "tokenizer.ggml.model",
];

#[derive(Debug, Clone)]
pub struct GgufSummary {
    pub version: u32,
    pub tensor_count: u64,
    pub kv_count: u64,
    pub architecture: Option<String>,
    pub name: Option<String>,
    pub quantization: Option<String>,
    pub dominant_tensor_type: Option<String>,
    pub parameter_count: Option<u64>,
    pub alignment: u32,
    pub file_size: u64,
    pub kv_preview: BTreeMap<String, String>,
    /// Set when the tensor-data section implied by tensor offsets/sizes
    /// extends past the actual file size for the tensor types we know how
    /// to size. `None` means the check could not be performed (unknown
    /// tensor types present), not that the file is confirmed intact.
    pub size_consistency_checked: bool,
}

pub fn inspect(path: &Path) -> Result<GgufSummary, GgufError> {
    let file_size = std::fs::metadata(path)?.len();
    let file = File::open(path)?;
    let mut r = BufReader::new(file);

    let mut magic = [0u8; 4];
    read_exact_ctx(&mut r, &mut magic, "magic")?;
    if magic != MAGIC {
        return Err(GgufError::BadMagic { found: magic });
    }

    let version = read_u32(&mut r, "version")?;
    if !SUPPORTED_VERSIONS.contains(&version) {
        return Err(GgufError::UnsupportedVersion(version));
    }

    let tensor_count = read_u64(&mut r, "tensor_count")?;
    if tensor_count > MAX_TENSOR_COUNT {
        return Err(GgufError::CountOutOfRange {
            field: "tensor_count".into(),
            value: tensor_count,
            limit: MAX_TENSOR_COUNT,
        });
    }

    let kv_count = read_u64(&mut r, "kv_count")?;
    if kv_count > MAX_KV_COUNT {
        return Err(GgufError::CountOutOfRange {
            field: "kv_count".into(),
            value: kv_count,
            limit: MAX_KV_COUNT,
        });
    }

    let mut kv_preview = BTreeMap::new();
    for i in 0..kv_count {
        let key = read_gguf_string(&mut r, file_size, &format!("kv[{i}].key"))?;
        let value_type = read_u32(&mut r, &format!("kv[{i}].value_type"))?;
        let summary = read_value_summary(&mut r, value_type, file_size, 0)?;
        if PREVIEW_KEYS.contains(&key.as_str()) {
            kv_preview.insert(key, summary);
        }
    }

    let mut parameter_count: u64 = 0;
    let mut parameter_count_overflowed = false;
    let mut tensor_type_counts: BTreeMap<u32, u64> = BTreeMap::new();
    let mut all_tensor_types_known = true;
    let mut max_data_end: u64 = 0;

    for i in 0..tensor_count {
        let _name = read_gguf_string(&mut r, file_size, &format!("tensor[{i}].name"))?;
        let n_dims = read_u32(&mut r, &format!("tensor[{i}].n_dims"))?;
        if n_dims > MAX_DIMS {
            return Err(GgufError::CountOutOfRange {
                field: format!("tensor[{i}].n_dims"),
                value: n_dims as u64,
                limit: MAX_DIMS as u64,
            });
        }
        let mut elems: u64 = 1;
        for d in 0..n_dims {
            let dim = read_u64(&mut r, &format!("tensor[{i}].dims[{d}]"))?;
            elems = elems.saturating_mul(dim.max(1));
        }
        let ggml_type = read_u32(&mut r, &format!("tensor[{i}].type"))?;
        let offset = read_u64(&mut r, &format!("tensor[{i}].offset"))?;

        *tensor_type_counts.entry(ggml_type).or_insert(0) += 1;
        match parameter_count.checked_add(elems) {
            Some(v) => parameter_count = v,
            None => parameter_count_overflowed = true,
        }

        match tensor_byte_size(ggml_type, elems) {
            Some(bytes) => max_data_end = max_data_end.max(offset.saturating_add(bytes)),
            None => all_tensor_types_known = false,
        }
    }

    let alignment: u32 = kv_preview
        .get("general.alignment")
        .and_then(|s| s.parse().ok())
        .unwrap_or(32);

    let data_start = r.stream_position()?;
    let aligned_data_start = align_up(data_start, alignment as u64);

    let size_consistency_checked = all_tensor_types_known && tensor_count > 0;
    if size_consistency_checked {
        let implied_min_size = aligned_data_start.saturating_add(max_data_end);
        if implied_min_size > file_size {
            return Err(GgufError::Truncated {
                context: "tensor data section (computed from tensor offsets/sizes)".into(),
                expected: implied_min_size as usize,
                actual: file_size as usize,
            });
        }
    }

    let dominant_tensor_type = tensor_type_counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(t, _)| ggml_type_name(t));

    let quantization = kv_preview
        .get("general.file_type")
        .and_then(|s| s.parse::<u32>().ok())
        .map(file_type_name)
        .or_else(|| dominant_tensor_type.clone());

    Ok(GgufSummary {
        version,
        tensor_count,
        kv_count,
        architecture: kv_preview.get("general.architecture").cloned(),
        name: kv_preview.get("general.name").cloned(),
        quantization,
        dominant_tensor_type,
        parameter_count: if parameter_count_overflowed {
            None
        } else {
            Some(parameter_count)
        },
        alignment,
        file_size,
        kv_preview,
        size_consistency_checked,
    })
}

fn align_up(value: u64, alignment: u64) -> u64 {
    if alignment == 0 {
        return value;
    }
    value.div_ceil(alignment) * alignment
}

/// Byte size of a tensor with `elems` elements stored as `ggml_type`.
/// Covers the common float and k-quant/legacy-quant block formats used by
/// mainstream GGUF models. Returns `None` for types we don't have a block
/// size table entry for, which disables the size-consistency check rather
/// than guessing.
fn tensor_byte_size(ggml_type: u32, elems: u64) -> Option<u64> {
    // (block_size_in_elems, bytes_per_block)
    let (block, bytes_per_block): (u64, u64) = match ggml_type {
        0 => (1, 4),      // F32
        1 => (1, 2),      // F16
        2 => (32, 18),    // Q4_0
        3 => (32, 20),    // Q4_1
        6 => (32, 22),    // Q5_0
        7 => (32, 24),    // Q5_1
        8 => (32, 34),    // Q8_0
        9 => (32, 40),    // Q8_1
        10 => (256, 84),  // Q2_K
        11 => (256, 110), // Q3_K
        12 => (256, 144), // Q4_K
        13 => (256, 176), // Q5_K
        14 => (256, 210), // Q6_K
        15 => (256, 292), // Q8_K
        30 => (1, 2),     // BF16
        _ => return None,
    };
    let blocks = elems.div_ceil(block.max(1));
    Some(blocks.saturating_mul(bytes_per_block))
}

fn ggml_type_name(t: u32) -> String {
    match t {
        0 => "F32",
        1 => "F16",
        2 => "Q4_0",
        3 => "Q4_1",
        6 => "Q5_0",
        7 => "Q5_1",
        8 => "Q8_0",
        9 => "Q8_1",
        10 => "Q2_K",
        11 => "Q3_K",
        12 => "Q4_K",
        13 => "Q5_K",
        14 => "Q6_K",
        15 => "Q8_K",
        30 => "BF16",
        other => return format!("GGML_TYPE_{other}"),
    }
    .to_string()
}

/// Maps the legacy `general.file_type` enum (still emitted by most
/// converters) to a friendly name.
fn file_type_name(ft: u32) -> String {
    match ft {
        0 => "ALL_F32",
        1 => "MOSTLY_F16",
        2 => "MOSTLY_Q4_0",
        3 => "MOSTLY_Q4_1",
        7 => "MOSTLY_Q8_0",
        8 => "MOSTLY_Q5_0",
        9 => "MOSTLY_Q5_1",
        10 => "MOSTLY_Q2_K",
        11 => "MOSTLY_Q3_K_S",
        12 => "MOSTLY_Q3_K_M",
        13 => "MOSTLY_Q3_K_L",
        14 => "MOSTLY_Q4_K_S",
        15 => "MOSTLY_Q4_K_M",
        16 => "MOSTLY_Q5_K_S",
        17 => "MOSTLY_Q5_K_M",
        18 => "MOSTLY_Q6_K",
        32 => "MOSTLY_BF16",
        other => return format!("FILE_TYPE_{other}"),
    }
    .to_string()
}

fn read_exact_ctx<R: Read>(r: &mut R, buf: &mut [u8], context: &str) -> Result<(), GgufError> {
    r.read_exact(buf).map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            GgufError::Truncated {
                context: context.to_string(),
                expected: buf.len(),
                actual: 0,
            }
        } else {
            GgufError::Io(e)
        }
    })
}

fn read_u32<R: Read>(r: &mut R, context: &str) -> Result<u32, GgufError> {
    let mut buf = [0u8; 4];
    read_exact_ctx(r, &mut buf, context)?;
    Ok(u32::from_le_bytes(buf))
}

fn read_u64<R: Read>(r: &mut R, context: &str) -> Result<u64, GgufError> {
    let mut buf = [0u8; 8];
    read_exact_ctx(r, &mut buf, context)?;
    Ok(u64::from_le_bytes(buf))
}

fn read_gguf_string<R: Read>(
    r: &mut R,
    file_size: u64,
    context: &str,
) -> Result<String, GgufError> {
    let len = read_u64(r, &format!("{context}.len"))?;
    if len > MAX_STRING_LEN || len > file_size {
        return Err(GgufError::CountOutOfRange {
            field: context.to_string(),
            value: len,
            limit: MAX_STRING_LEN.min(file_size),
        });
    }
    let mut buf = vec![0u8; len as usize];
    read_exact_ctx(r, &mut buf, context)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// Reads a value of `value_type`, advancing the stream past all of it, and
/// returns a short string representation. Always the single source of truth
/// for "how many bytes does this value occupy" so preview extraction and
/// skip-only paths can never disagree about stream position.
fn read_value_summary<R: Read + Seek>(
    r: &mut R,
    value_type: u32,
    file_size: u64,
    depth: u32,
) -> Result<String, GgufError> {
    if depth > 4 {
        return Err(GgufError::Malformed("array nesting too deep".into()));
    }

    match value_type {
        GGUF_TYPE_U8 | GGUF_TYPE_I8 | GGUF_TYPE_BOOL => {
            let mut b = [0u8; 1];
            read_exact_ctx(r, &mut b, "scalar u8/i8/bool")?;
            Ok(b[0].to_string())
        }
        GGUF_TYPE_U16 | GGUF_TYPE_I16 => {
            let mut b = [0u8; 2];
            read_exact_ctx(r, &mut b, "scalar u16/i16")?;
            Ok(u16::from_le_bytes(b).to_string())
        }
        GGUF_TYPE_U32 => Ok(read_u32(r, "scalar u32")?.to_string()),
        GGUF_TYPE_I32 => Ok(read_u32(r, "scalar i32")?.to_string()),
        GGUF_TYPE_F32 => {
            Ok(f32::from_le_bytes(read_u32(r, "scalar f32")?.to_le_bytes()).to_string())
        }
        GGUF_TYPE_U64 => Ok(read_u64(r, "scalar u64")?.to_string()),
        GGUF_TYPE_I64 => Ok(read_u64(r, "scalar i64")?.to_string()),
        GGUF_TYPE_F64 => {
            Ok(f64::from_le_bytes(read_u64(r, "scalar f64")?.to_le_bytes()).to_string())
        }
        GGUF_TYPE_STRING => read_gguf_string(r, file_size, "scalar string"),
        GGUF_TYPE_ARRAY => {
            let elem_type = read_u32(r, "array.elem_type")?;
            let arr_len = read_u64(r, "array.len")?;
            if arr_len > MAX_ARRAY_LEN {
                return Err(GgufError::CountOutOfRange {
                    field: "array.len".into(),
                    value: arr_len,
                    limit: MAX_ARRAY_LEN,
                });
            }
            if let Some(elem_size) = fixed_scalar_size(elem_type) {
                let total = arr_len.saturating_mul(elem_size as u64);
                if total > file_size {
                    return Err(GgufError::CountOutOfRange {
                        field: "array (fixed-width) total bytes".into(),
                        value: total,
                        limit: file_size,
                    });
                }
                r.seek(SeekFrom::Current(total as i64))
                    .map_err(GgufError::Io)?;
            } else {
                for _ in 0..arr_len {
                    read_value_summary(r, elem_type, file_size, depth + 1)?;
                }
            }
            Ok(format!("<array: {arr_len} x type {elem_type}>"))
        }
        other => Err(GgufError::Malformed(format!("unknown value_type {other}"))),
    }
}

fn fixed_scalar_size(t: u32) -> Option<u32> {
    match t {
        GGUF_TYPE_U8 | GGUF_TYPE_I8 | GGUF_TYPE_BOOL => Some(1),
        GGUF_TYPE_U16 | GGUF_TYPE_I16 => Some(2),
        GGUF_TYPE_U32 | GGUF_TYPE_I32 | GGUF_TYPE_F32 => Some(4),
        GGUF_TYPE_U64 | GGUF_TYPE_I64 | GGUF_TYPE_F64 => Some(8),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Minimal fixture builder mirroring the real GGUF byte layout, kept
    /// separate from the parser so tests exercise the actual wire format
    /// rather than round-tripping through parser internals.
    struct Fixture {
        buf: Vec<u8>,
    }

    impl Fixture {
        fn new() -> Self {
            let mut buf = Vec::new();
            buf.extend_from_slice(b"GGUF");
            Self { buf }
        }
        fn u32(mut self, v: u32) -> Self {
            self.buf.extend_from_slice(&v.to_le_bytes());
            self
        }
        fn u64(mut self, v: u64) -> Self {
            self.buf.extend_from_slice(&v.to_le_bytes());
            self
        }
        fn string(mut self, s: &str) -> Self {
            self.buf.extend_from_slice(&(s.len() as u64).to_le_bytes());
            self.buf.extend_from_slice(s.as_bytes());
            self
        }
        fn kv_string(self, key: &str, value: &str) -> Self {
            self.string(key).u32(GGUF_TYPE_STRING).string(value)
        }
        fn kv_u32(self, key: &str, value: u32) -> Self {
            self.string(key).u32(GGUF_TYPE_U32).u32(value)
        }
        fn raw(mut self, bytes: &[u8]) -> Self {
            self.buf.extend_from_slice(bytes);
            self
        }
        fn write_to(self, path: &Path) {
            File::create(path).unwrap().write_all(&self.buf).unwrap();
        }
    }

    /// Each call gets its own directory (pid + nanosecond timestamp) - tests
    /// run in parallel and each cleans up its own directory with
    /// `remove_dir_all`, so sharing one directory across tests would let one
    /// test's cleanup delete another test's still-in-use fixture file.
    fn tmp_path(name: &str) -> std::path::PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("brute-gguf-test-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    /// Builds a minimal but fully valid GGUF: 2 KV pairs, 1 F32 tensor of
    /// 4 elements (16 bytes), padded/aligned data section actually present.
    fn valid_minimal(alignment: u32) -> Fixture {
        let mut f = Fixture::new()
            .u32(3) // version
            .u64(1) // tensor_count
            .u64(2) // kv_count
            .kv_string("general.architecture", "llama")
            .kv_u32("general.alignment", alignment)
            // tensor[0]: name, n_dims=1, dims=[4], type=F32(0), offset=0
            .string("weight")
            .u32(1)
            .u64(4)
            .u32(0)
            .u64(0);

        // Compute data start and pad to alignment, then append 16 bytes of data.
        let unpadded = f.buf.len() as u64;
        let padded = align_up(unpadded, alignment as u64);
        let padding = vec![0u8; (padded - unpadded) as usize];
        f = f.raw(&padding).raw(&[0u8; 16]);
        f
    }

    #[test]
    fn parses_valid_minimal_file() {
        let path = tmp_path("valid.gguf");
        valid_minimal(32).write_to(&path);

        let summary = inspect(&path).expect("should parse");
        assert_eq!(summary.version, 3);
        assert_eq!(summary.tensor_count, 1);
        assert_eq!(summary.architecture.as_deref(), Some("llama"));
        assert_eq!(summary.parameter_count, Some(4));
        assert_eq!(summary.alignment, 32);
        assert!(summary.size_consistency_checked);

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn rejects_bad_magic() {
        let path = tmp_path("bad_magic.gguf");
        let mut buf = valid_minimal(32).buf;
        buf[0] = b'X';
        std::fs::write(&path, &buf).unwrap();

        let err = inspect(&path).unwrap_err();
        assert!(matches!(err, GgufError::BadMagic { .. }));
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn rejects_unsupported_version() {
        let path = tmp_path("bad_version.gguf");
        let f = Fixture::new().u32(99).u64(0).u64(0);
        f.write_to(&path);

        let err = inspect(&path).unwrap_err();
        assert!(matches!(err, GgufError::UnsupportedVersion(99)));
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn rejects_truncated_mid_kv() {
        let path = tmp_path("truncated.gguf");
        let mut buf = valid_minimal(32).buf;
        buf.truncate(buf.len() - 30); // chop off tail, well into the structured section
        std::fs::write(&path, &buf).unwrap();

        let err = inspect(&path).unwrap_err();
        assert!(
            matches!(err, GgufError::Truncated { .. }),
            "expected Truncated, got {err:?}"
        );
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn rejects_file_shorter_than_declared_tensor_data() {
        let path = tmp_path("short_data.gguf");
        let mut buf = valid_minimal(32).buf;
        // Drop the last 16 bytes (the tensor's data payload) without
        // touching the header/kv/tensor-info section - this simulates a
        // file that got cut off while the data section was being written.
        buf.truncate(buf.len() - 16);
        std::fs::write(&path, &buf).unwrap();

        let err = inspect(&path).unwrap_err();
        assert!(matches!(err, GgufError::Truncated { .. }));
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn rejects_hostile_kv_count() {
        let path = tmp_path("hostile_kv.gguf");
        let f = Fixture::new().u32(3).u64(0).u64(u64::MAX);
        f.write_to(&path);

        let err = inspect(&path).unwrap_err();
        assert!(matches!(err, GgufError::CountOutOfRange { .. }));
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn rejects_hostile_string_length() {
        let path = tmp_path("hostile_string.gguf");
        let f = Fixture::new()
            .u32(3)
            .u64(0) // tensor_count
            .u64(1) // kv_count
            .u64(u64::MAX) // key length claim
            .raw(b"short");
        f.write_to(&path);

        let err = inspect(&path).unwrap_err();
        assert!(matches!(err, GgufError::CountOutOfRange { .. }));
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }
}
