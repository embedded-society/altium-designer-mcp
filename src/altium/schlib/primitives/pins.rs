//! `SchLib` pin primitives (pin record, decorations, orientation, electrical type).

#[allow(clippy::wildcard_imports)] // sibling primitive types
use super::*;

/// A schematic symbol pin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)] // Pin flags match Altium binary format
pub struct Pin {
    /// Pin name (e.g., "VCC", "GND", "IN").
    pub name: String,

    /// Pin designator (e.g., "1", "2", "A1").
    pub designator: String,

    /// X position in schematic units (10 units = 1 grid).
    pub x: i32,

    /// Y position in schematic units.
    pub y: i32,

    /// Pin length in schematic units.
    pub length: i32,

    /// Pin orientation.
    #[serde(default)]
    pub orientation: PinOrientation,

    /// Electrical type.
    #[serde(default)]
    pub electrical_type: PinElectricalType,

    /// Whether the pin is hidden.
    #[serde(default)]
    pub hidden: bool,

    /// Whether to show the pin name.
    #[serde(default = "default_true")]
    pub show_name: bool,

    /// Whether to show the pin designator.
    #[serde(default = "default_true")]
    pub show_designator: bool,

    /// Pin description.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,

    /// Owner part ID (for multi-part symbols).
    #[serde(default = "default_owner_part")]
    pub owner_part_id: i32,

    /// Owner part display mode (alternate-view index). Stored in the binary pin
    /// record's own byte at offset 7 — distinct from the `OwnerPartDisplayMode`
    /// that `PR-14` put on the `SchLib` *shapes* (a text-record parameter). Altium
    /// emits `0` for a normal pin; preserved on round-trip.
    #[serde(default)]
    pub owner_part_display_mode: i32,

    /// Pin colour (BGR format).
    #[serde(default)]
    pub colour: u32,

    /// Whether the pin is graphically locked.
    #[serde(default)]
    pub graphically_locked: bool,

    /// Whether the pin is not accessible for selection in Altium's editor
    /// (conglomerate bit `0x20`); preserved on round-trip (#113).
    #[serde(default)]
    pub is_not_accessible: bool,

    /// Symbol decoration on the inner edge (closest to component body).
    #[serde(default)]
    pub symbol_inner_edge: PinSymbol,

    /// Symbol decoration on the outer edge (furthest from component body).
    #[serde(default)]
    pub symbol_outer_edge: PinSymbol,

    /// Symbol decoration inside the pin line.
    #[serde(default)]
    pub symbol_inside: PinSymbol,

    /// Symbol decoration outside the pin line.
    #[serde(default)]
    pub symbol_outside: PinSymbol,

    /// Pin formal type byte. Altium emits `1` for a normal pin; preserved on
    /// round-trip. Non-default values come from Altium-authored files.
    #[serde(default = "default_formal_type")]
    pub formal_type: u8,

    /// Pin swap-id group (empty for a from-scratch pin).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub swap_id_group: String,

    /// Pin part-and-sequence swap id. Altium's default for a fresh pin is `|&|`.
    #[serde(default = "default_part_and_sequence")]
    pub part_and_sequence: String,

    /// Pin default value (empty for a from-scratch pin).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub default_value: String,

    /// Symbol line width index. Lives in the per-component `PinSymbolLineWidth`
    /// auxiliary OLE stream (a `SYMBOL_LINEWIDTH=N` parameter, one entry per
    /// non-default pin), NOT in the binary pin record. `0` is the default and
    /// causes no stream entry to be written, so a from-scratch pin is
    /// byte-identical to Altium's output.
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub symbol_line_width: i32,

    /// Fractional companion of `x` / `y` / `length`, in 1/100000-DXP units,
    /// preserved from the per-component `PinFrac` auxiliary OLE stream. The
    /// binary pin record stores only the integer DXP part (i16); a pin sitting
    /// off the integer grid carries its sub-unit remainder here so it
    /// round-trips. `None` for an on-grid pin (the common case, incl. the whole
    /// golden), which writes no `PinFrac` entry → byte-identical.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frac: Option<PinFrac>,
}

/// Fractional companion coordinates for a [`Pin`].
///
/// Mirrors the three little-endian `i32` values in a `PinFrac` auxiliary-stream
/// entry (`frac_x`, `frac_y`, `frac_length`). Each is a sub-DXP-unit remainder
/// in 1/100000 units, matching Altium's `raw = num * 100000 + frac` convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PinFrac {
    /// Fractional part of the pin's X coordinate (1/100000 DXP units).
    #[serde(default)]
    pub x: i32,
    /// Fractional part of the pin's Y coordinate (1/100000 DXP units).
    #[serde(default)]
    pub y: i32,
    /// Fractional part of the pin's length (1/100000 DXP units).
    #[serde(default)]
    pub length: i32,
}

impl PinFrac {
    /// Returns `true` when every fractional part is zero (an on-grid pin that
    /// needs no `PinFrac` stream entry).
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.x == 0 && self.y == 0 && self.length == 0
    }
}

const fn default_formal_type() -> u8 {
    1
}

fn default_part_and_sequence() -> String {
    "|&|".to_string()
}

impl Pin {
    /// Creates a new pin with the given name and designator.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        designator: impl Into<String>,
        x: i32,
        y: i32,
        length: i32,
        orientation: PinOrientation,
    ) -> Self {
        Self {
            name: name.into(),
            designator: designator.into(),
            x,
            y,
            length,
            orientation,
            electrical_type: PinElectricalType::Passive,
            hidden: false,
            show_name: true,
            show_designator: true,
            description: String::new(),
            owner_part_id: 1,
            owner_part_display_mode: 0,
            colour: 0,
            graphically_locked: false,
            is_not_accessible: false,
            symbol_inner_edge: PinSymbol::None,
            symbol_outer_edge: PinSymbol::None,
            symbol_inside: PinSymbol::None,
            symbol_outside: PinSymbol::None,
            formal_type: 1,
            swap_id_group: String::new(),
            part_and_sequence: "|&|".to_string(),
            default_value: String::new(),
            symbol_line_width: 0,
            frac: None,
        }
    }
}

/// Pin symbol decoration (visual indicators on pin graphics).
///
/// These decorations appear at different positions on the pin to indicate
/// electrical characteristics or signal flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinSymbol {
    /// No decoration.
    #[default]
    None,
    /// Inversion dot (bubble).
    Dot,
    /// Right-to-left signal flow arrow.
    RightLeftSignalFlow,
    /// Clock input indicator.
    Clock,
    /// Active low input bar.
    ActiveLowInput,
    /// Analog signal input.
    AnalogSignalIn,
    /// Not a logic connection.
    NotLogicConnection,
    /// Postponed output.
    PostponedOutput,
    /// Open collector output.
    OpenCollector,
    /// High impedance.
    HiZ,
    /// High current.
    HighCurrent,
    /// Pulse.
    Pulse,
    /// Schmitt trigger input.
    Schmitt,
    /// Active low output bar.
    ActiveLowOutput,
    /// Open collector with pull-up.
    OpenCollectorPullUp,
    /// Open emitter output.
    OpenEmitter,
    /// Open emitter with pull-up.
    OpenEmitterPullUp,
    /// Digital signal input.
    DigitalSignalIn,
    /// Shift left.
    ShiftLeft,
    /// Open output.
    OpenOutput,
    /// Left-to-right signal flow arrow.
    LeftRightSignalFlow,
    /// Bidirectional signal flow.
    BidirectionalSignalFlow,
}

impl PinSymbol {
    /// Creates from Altium symbol ID.
    #[must_use]
    pub const fn from_id(id: u8) -> Self {
        match id {
            1 => Self::Dot,
            2 => Self::RightLeftSignalFlow,
            3 => Self::Clock,
            4 => Self::ActiveLowInput,
            5 => Self::AnalogSignalIn,
            6 => Self::NotLogicConnection,
            7 => Self::PostponedOutput,
            8 => Self::OpenCollector,
            9 => Self::HiZ,
            10 => Self::HighCurrent,
            11 => Self::Pulse,
            12 => Self::Schmitt,
            13 => Self::ActiveLowOutput,
            14 => Self::OpenCollectorPullUp,
            15 => Self::OpenEmitter,
            16 => Self::OpenEmitterPullUp,
            17 => Self::DigitalSignalIn,
            18 => Self::ShiftLeft,
            19 => Self::OpenOutput,
            20 => Self::LeftRightSignalFlow,
            21 => Self::BidirectionalSignalFlow,
            _ => Self::None,
        }
    }

    /// Returns the Altium symbol ID.
    #[must_use]
    pub const fn to_id(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Dot => 1,
            Self::RightLeftSignalFlow => 2,
            Self::Clock => 3,
            Self::ActiveLowInput => 4,
            Self::AnalogSignalIn => 5,
            Self::NotLogicConnection => 6,
            Self::PostponedOutput => 7,
            Self::OpenCollector => 8,
            Self::HiZ => 9,
            Self::HighCurrent => 10,
            Self::Pulse => 11,
            Self::Schmitt => 12,
            Self::ActiveLowOutput => 13,
            Self::OpenCollectorPullUp => 14,
            Self::OpenEmitter => 15,
            Self::OpenEmitterPullUp => 16,
            Self::DigitalSignalIn => 17,
            Self::ShiftLeft => 18,
            Self::OpenOutput => 19,
            Self::LeftRightSignalFlow => 20,
            Self::BidirectionalSignalFlow => 21,
        }
    }
}

/// Pin orientation (direction the pin points).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinOrientation {
    /// Pin points right (connection on left side).
    #[default]
    Right,
    /// Pin points left (connection on right side).
    Left,
    /// Pin points up (connection on bottom).
    Up,
    /// Pin points down (connection on top).
    Down,
}

impl PinOrientation {
    /// Creates orientation from rotation and flip flags.
    #[must_use]
    pub const fn from_flags(rotated: bool, flipped: bool) -> Self {
        match (rotated, flipped) {
            (false, false) => Self::Right,
            (false, true) => Self::Left,
            (true, false) => Self::Up,
            (true, true) => Self::Down,
        }
    }

    /// Returns the rotation and flip flags for this orientation.
    #[must_use]
    pub const fn to_flags(self) -> (bool, bool) {
        match self {
            Self::Right => (false, false),
            Self::Left => (false, true),
            Self::Up => (true, false),
            Self::Down => (true, true),
        }
    }
}

/// Pin electrical type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinElectricalType {
    /// Input pin.
    Input,
    /// Bidirectional pin (input/output).
    #[serde(alias = "input_output")]
    Bidirectional,
    /// Output pin.
    Output,
    /// Open collector output.
    OpenCollector,
    /// Passive component (resistor, capacitor).
    #[default]
    Passive,
    /// High impedance / tri-state.
    HiZ,
    /// Open emitter output.
    OpenEmitter,
    /// Power pin (VCC, GND).
    Power,
}

impl PinElectricalType {
    /// Creates from Altium electrical type ID.
    #[must_use]
    pub const fn from_id(id: u8) -> Self {
        match id {
            0 => Self::Input,
            1 => Self::Bidirectional,
            2 => Self::Output,
            3 => Self::OpenCollector,
            5 => Self::HiZ,
            6 => Self::OpenEmitter,
            7 => Self::Power,
            // 4 and unknown IDs default to Passive
            _ => Self::Passive,
        }
    }

    /// Returns the Altium electrical type ID.
    #[must_use]
    pub const fn to_id(self) -> u8 {
        match self {
            Self::Input => 0,
            Self::Bidirectional => 1,
            Self::Output => 2,
            Self::OpenCollector => 3,
            Self::Passive => 4,
            Self::HiZ => 5,
            Self::OpenEmitter => 6,
            Self::Power => 7,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_orientation_flags() {
        assert_eq!(
            PinOrientation::from_flags(false, false),
            PinOrientation::Right
        );
        assert_eq!(
            PinOrientation::from_flags(false, true),
            PinOrientation::Left
        );
        assert_eq!(PinOrientation::from_flags(true, false), PinOrientation::Up);
        assert_eq!(PinOrientation::from_flags(true, true), PinOrientation::Down);

        assert_eq!(PinOrientation::Right.to_flags(), (false, false));
        assert_eq!(PinOrientation::Left.to_flags(), (false, true));
    }

    #[test]
    fn pin_electrical_type_roundtrip() {
        for id in 0..8 {
            let etype = PinElectricalType::from_id(id);
            assert_eq!(etype.to_id(), id);
        }
    }

    #[test]
    fn pin_electrical_type_unknown_id_defaults_to_passive() {
        assert_eq!(PinElectricalType::from_id(99), PinElectricalType::Passive);
    }

    #[test]
    fn pin_orientation_up_and_down_flags() {
        assert_eq!(PinOrientation::Up.to_flags(), (true, false));
        assert_eq!(PinOrientation::Down.to_flags(), (true, true));
    }

    #[test]
    fn pin_symbol_round_trips_all_ids() {
        for id in 0..=21 {
            assert_eq!(
                PinSymbol::from_id(id).to_id(),
                id,
                "id {id} did not round-trip"
            );
        }
        // Out-of-range ids fall back to the undecorated symbol (id 0).
        assert_eq!(PinSymbol::from_id(99), PinSymbol::None);
        assert_eq!(PinSymbol::from_id(255), PinSymbol::None);
        assert_eq!(PinSymbol::None.to_id(), 0);
    }
}
