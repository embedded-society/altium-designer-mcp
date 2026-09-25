//! A pad's or via's own polygon-connect style ([`PolygonConnect`]): how a
//! polygon pour on its net joins it.
//!
//! Both records keep it the same way — an entry count, an entry size of 30
//! and that many 30-byte entries — at different offsets:
//!
//! | Record | Count (`i32`) | Entry size (`i32`) | Entries |
//! |--------|---------------|--------------------|---------|
//! | Pad main block | @176 | @194 | @198 |
//! | Via block | @300 | @304 | @308 |
//!
//! A pad without an entry ends at 194 bytes, with no entry size; a via always
//! carries the entry size, and every via field that follows its entries moves
//! along by 30 bytes per entry. Only the first entry is modelled; every
//! sample holds at most one (`manual/thermal_relief.PcbLib` for pads,
//! `manual/plane_and_via.PcbLib` for vias).

use super::primitives::{PolygonConnect, PowerPlaneConnectStyle};
use super::units::{from_mm, to_mm};
use crate::altium::bytes::read_i32_le as read_i32;

/// Length of one entry.
pub const ENTRY_LEN: usize = 30;

/// Where a record keeps its entries.
pub struct Layout {
    /// Offset of the `i32` entry count.
    count_at: usize,
    /// Offset of the `i32` entry size (30).
    size_at: usize,
    /// Offset of the first entry.
    entries_at: usize,
    /// Whether the entry size is written only while there are entries.
    size_only_with_entries: bool,
}

/// A pad's main block.
pub const PAD: Layout = Layout {
    count_at: 176,
    size_at: 194,
    entries_at: 198,
    size_only_with_entries: true,
};

/// A via's block.
pub const VIA: Layout = Layout {
    count_at: 300,
    size_at: 304,
    entries_at: 308,
    size_only_with_entries: false,
};

/// The entry AD24 writes for a pad whose Thermal Relief box is ticked and
/// nothing else is changed (`manual/thermal_relief.PcbLib`).
#[rustfmt::skip]
pub const PAD_ENTRY_TEMPLATE: [u8; ENTRY_LEN] = [
    0x00,0x00,0x00,0x00,                      // reserved
    0x01, 0x00,                               // present; style Relief
    0xA0,0x86,0x01,0x00, 0xA0,0x86,0x01,0x00, // air gap, conductor width: 10 mil
    0x01, 0x04,                               // 90 degrees; 4 conductors
    0x00,0x00,0x00, 0x01,0x00,0x00,0x00,      // not modelled
    0x00,                                     // Auto conductors off
    0xF0,0x49,0x02,0x00, 0x00, 0x00,          // min distance 15 mil, unticked; reserved
];

/// The entry an older Altium wrote on every via it saved
/// (`manual/plane_and_via.PcbLib`); it differs from the pad's only in the
/// four bytes this crate does not model.
#[rustfmt::skip]
pub const VIA_ENTRY_TEMPLATE: [u8; ENTRY_LEN] = [
    0x00,0x00,0x00,0x00,
    0x01, 0x00,
    0xA0,0x86,0x01,0x00, 0xA0,0x86,0x01,0x00,
    0x01, 0x04,
    0x00,0x00,0x00, 0x40,0x0D,0x03,0x00,
    0x00,
    0xF0,0x49,0x02,0x00, 0x00, 0x00,
];

/// The number of entries `record` holds: 0 unless the count is positive, the
/// entry size is 30 and the record is long enough to hold them all.
#[must_use]
pub fn entry_count(record: &[u8], layout: &Layout) -> usize {
    let count = read_i32(record, layout.count_at).and_then(|n| usize::try_from(n).ok());
    match (count, read_i32(record, layout.size_at)) {
        (Some(n), Some(30)) if n > 0 && record.len() >= layout.entries_at + n * ENTRY_LEN => n,
        _ => 0,
    }
}

/// Reads the record's first entry, or `None` when it holds none or its
/// present byte is clear (the bytes still ride in the record as read).
#[must_use]
pub fn read(record: &[u8], layout: &Layout) -> Option<PolygonConnect> {
    if entry_count(record, layout) == 0 {
        return None;
    }
    let entry = record.get(layout.entries_at..layout.entries_at + ENTRY_LEN)?;
    if *entry.get(4)? != 1 {
        return None;
    }
    Some(PolygonConnect {
        style: PowerPlaneConnectStyle::from_id(*entry.get(5)?),
        air_gap: to_mm(read_i32(entry, 6)?),
        conductor_width: to_mm(read_i32(entry, 10)?),
        rotation: if *entry.get(14)? == 0 { 45 } else { 90 },
        conductors: *entry.get(15)?,
        auto_conductors: *entry.get(23)? != 0,
        min_distance: to_mm(read_i32(entry, 24)?),
        min_distance_enabled: *entry.get(28)? != 0,
    })
}

/// Writes `connect` into the record's first entry, adding one from
/// `template` when the record holds none. With `None`, takes the entries out
/// when the first is a present one, so the primitive follows the design rules
/// as it does in Altium with the box unticked. A pad record is first cut to
/// 194 bytes, where AD24 ends a pad without an entry — the from-scratch
/// template runs to 202.
pub fn write(
    record: &mut Vec<u8>,
    layout: &Layout,
    connect: Option<&PolygonConnect>,
    template: &[u8; ENTRY_LEN],
) {
    let count = entry_count(record, layout);
    let Some(connect) = connect else {
        if count > 0 && record[layout.entries_at + 4] == 1 {
            let from = if layout.size_only_with_entries {
                layout.size_at
            } else {
                layout.entries_at
            };
            record.drain(from..layout.entries_at + count * ENTRY_LEN);
            set_count(record, layout, 0);
        }
        return;
    };
    if count == 0 {
        if layout.size_only_with_entries {
            record.resize(layout.size_at, 0);
            record.extend_from_slice(&30_i32.to_le_bytes());
        }
        record.splice(
            layout.entries_at..layout.entries_at,
            template.iter().copied(),
        );
        set_count(record, layout, 1);
    }
    let entry = &mut record[layout.entries_at..layout.entries_at + ENTRY_LEN];
    entry[4] = 1;
    entry[5] = connect.style.to_id();
    entry[6..10].copy_from_slice(&from_mm(connect.air_gap).to_le_bytes());
    entry[10..14].copy_from_slice(&from_mm(connect.conductor_width).to_le_bytes());
    entry[14] = u8::from(connect.rotation != 45);
    entry[15] = connect.conductors;
    entry[23] = u8::from(connect.auto_conductors);
    entry[24..28].copy_from_slice(&from_mm(connect.min_distance).to_le_bytes());
    entry[28] = u8::from(connect.min_distance_enabled);
}

/// Writes the entry count.
fn set_count(record: &mut [u8], layout: &Layout, count: i32) {
    record[layout.count_at..layout.count_at + 4].copy_from_slice(&count.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::{read, ENTRY_LEN, PAD, PAD_ENTRY_TEMPLATE};

    /// A counted entry whose present byte is clear is no override — the
    /// bytes still ride in the record as read, and a rewrite replays them.
    #[test]
    fn an_entry_that_is_not_present_reads_as_none() {
        let mut record = vec![0u8; PAD.entries_at + ENTRY_LEN];
        record[PAD.count_at..PAD.count_at + 4].copy_from_slice(&1_i32.to_le_bytes());
        record[PAD.size_at..PAD.size_at + 4].copy_from_slice(&30_i32.to_le_bytes());
        record[PAD.entries_at..].copy_from_slice(&PAD_ENTRY_TEMPLATE);
        assert!(
            read(&record, &PAD).is_some(),
            "the template entry is present"
        );

        record[PAD.entries_at + 4] = 0;
        assert_eq!(read(&record, &PAD), None);
    }
}
