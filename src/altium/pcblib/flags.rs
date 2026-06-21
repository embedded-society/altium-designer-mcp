//! Altium `PcbLib` primitive flag-word bits (common-header bytes 1–2).
//!
//! Shared by the writer's `encode_altium_flags` and the reader's `read_flags`
//! so the bit values are defined once instead of copied into both. These are
//! `PcbLib`-specific — `SchLib` pins use a different, unrelated 8-bit flag
//! table, so they intentionally do not live in `crate::altium`.

/// `FlagUnlocked` — set unless the primitive is locked.
pub(super) const ALT_FLAG_UNLOCKED: u16 = 0x0004;
/// `FlagSaved` — always set on a saved primitive (writer-side only).
pub(super) const ALT_FLAG_SAVED: u16 = 0x0008;
/// Solder-mask tenting on the top layer.
pub(super) const ALT_FLAG_TENTING_TOP: u16 = 0x0020;
/// Solder-mask tenting on the bottom layer.
pub(super) const ALT_FLAG_TENTING_BOTTOM: u16 = 0x0040;
/// Keepout primitive.
pub(super) const ALT_FLAG_KEEPOUT: u16 = 0x0200;
