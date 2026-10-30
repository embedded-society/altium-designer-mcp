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

    /// The ANSI code page a script widened this pin's name through, as Altium
    /// stored it: the binary record holds the name's UTF-8 bytes and the
    /// `PinWideText` entry those bytes read through this page (the golden's
    /// pins, scripted on a Windows-1250 machine, store `Résistance` as
    /// `RĂ©sistance` there). A rewrite stores the name that way again. `None`
    /// — a pin authored in the UI or here — stores the name in the code page,
    /// `?` for a character it cannot hold, with `PinWideText` holding it whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_code_page: Option<u32>,

    /// The text records Altium stores as this pin's children — a pin
    /// parameter (`RECORD=41`, `Name=Order`, say) whose `OwnerIndex` is the
    /// pin's stream-index — each as its `(key, value)` segments in stored
    /// order. They follow the pin in the stream, take no `IndexInSheet`
    /// slot, and go back verbatim after it with `OwnerIndex` set to the
    /// pin's new stream-index. They belong to the pin, never to the
    /// symbol's own parameters.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub owned_records: Vec<Vec<(String, String)>>,
    /// The pin's functions, in order: the peripheral roles a microcontroller
    /// pin can take (`UART1_TX`, `TIM2_CH1`, …), from which a schematic picks
    /// the one it uses. Altium's Functions property, stored in the symbol's
    /// `PinFunctionData` stream. Empty for a pin with none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub functions: Vec<String>,
    /// The pin's `PinFunctionData` entry as read, every `(key, value)` pair in
    /// stored order: the defined functions above, and what else Altium keeps
    /// there — the function a schematic selected
    /// (`PINSELECTEDFUNCTIONSCOUNT`, `PINSELECTEDFUNCTIONn`), say. The entry
    /// goes back as read while [`Self::functions`] still holds what it
    /// defined; otherwise the defined functions are rebuilt and the other
    /// pairs kept. Empty for a pin built from scratch.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub function_params: Vec<(String, String)>,
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
            name_code_page: None,
            owned_records: Vec::new(),
            functions: Vec::new(),
            function_params: Vec::new(),
        }
    }
}

/// Pin symbol decoration (visual indicators on pin graphics).
///
/// A pin's four decoration slots store Altium's `TIeeeSymbol` id, the same
/// enumeration as an IEEE symbol record (`RECORD=3`; table in
/// `docs/SCHLIB_FORMAT.md`). The pin dialog offers a subset of it — its ids
/// run with gaps (Postponed Output 8, Active Low Output 17, Open Collector
/// Pull Up 22, Left-Right Signal Flow 33) — but a slot can hold any glyph,
/// and an id past the table is kept as [`Self::Other`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinSymbol {
    /// No decoration (0).
    #[default]
    None,
    /// Inversion dot, a bubble (1).
    Dot,
    /// Right-to-left signal flow arrow (2).
    RightLeftSignalFlow,
    /// Clock input indicator (3).
    Clock,
    /// Active low input bar (4).
    ActiveLowInput,
    /// Analog signal input (5).
    AnalogSignalIn,
    /// Not a logic connection (6).
    NotLogicConnection,
    /// Shift right (7).
    ShiftRight,
    /// Postponed output (8).
    PostponedOutput,
    /// Open collector output (9).
    OpenCollector,
    /// High impedance (10).
    HiZ,
    /// High current (11).
    HighCurrent,
    /// Pulse (12).
    Pulse,
    /// Schmitt trigger input (13).
    Schmitt,
    /// Delay (14).
    Delay,
    /// Group line (15).
    GroupLine,
    /// Group binary (16).
    GroupBinary,
    /// Active low output bar (17).
    ActiveLowOutput,
    /// Pi (18).
    Pi,
    /// Greater than or equal (19).
    GreaterEqual,
    /// Less than or equal (20).
    LessEqual,
    /// Sigma (21).
    Sigma,
    /// Open collector with pull-up (22).
    OpenCollectorPullUp,
    /// Open emitter output (23).
    OpenEmitter,
    /// Open emitter with pull-up (24).
    OpenEmitterPullUp,
    /// Digital signal input (25).
    DigitalSignalIn,
    /// And (26).
    And,
    /// Invertor (27).
    Invertor,
    /// Or (28).
    Or,
    /// Exclusive or (29).
    Xor,
    /// Shift left (30).
    ShiftLeft,
    /// Input/output (31).
    InputOutput,
    /// Open circuit output (32).
    OpenOutput,
    /// Left-to-right signal flow arrow (33).
    LeftRightSignalFlow,
    /// Bidirectional signal flow (34).
    BidirectionalSignalFlow,
    /// An id past the table, kept as stored; written in JSON as the bare
    /// number.
    #[serde(untagged)]
    Other(u8),
}

impl PinSymbol {
    /// Every named decoration, in id order (`None` is 0).
    const NAMED: [Self; 35] = [
        Self::None,
        Self::Dot,
        Self::RightLeftSignalFlow,
        Self::Clock,
        Self::ActiveLowInput,
        Self::AnalogSignalIn,
        Self::NotLogicConnection,
        Self::ShiftRight,
        Self::PostponedOutput,
        Self::OpenCollector,
        Self::HiZ,
        Self::HighCurrent,
        Self::Pulse,
        Self::Schmitt,
        Self::Delay,
        Self::GroupLine,
        Self::GroupBinary,
        Self::ActiveLowOutput,
        Self::Pi,
        Self::GreaterEqual,
        Self::LessEqual,
        Self::Sigma,
        Self::OpenCollectorPullUp,
        Self::OpenEmitter,
        Self::OpenEmitterPullUp,
        Self::DigitalSignalIn,
        Self::And,
        Self::Invertor,
        Self::Or,
        Self::Xor,
        Self::ShiftLeft,
        Self::InputOutput,
        Self::OpenOutput,
        Self::LeftRightSignalFlow,
        Self::BidirectionalSignalFlow,
    ];

    /// Creates from Altium symbol ID.
    #[must_use]
    pub const fn from_id(id: u8) -> Self {
        if (id as usize) < Self::NAMED.len() {
            Self::NAMED[id as usize]
        } else {
            Self::Other(id)
        }
    }

    /// Returns the Altium symbol ID.
    #[must_use]
    pub fn to_id(self) -> u8 {
        if let Self::Other(id) = self {
            return id;
        }
        Self::NAMED
            .iter()
            .position(|named| *named == self)
            .and_then(|i| u8::try_from(i).ok())
            .unwrap_or(0)
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
        for id in 0..=u8::MAX {
            assert_eq!(
                PinSymbol::from_id(id).to_id(),
                id,
                "id {id} did not round-trip"
            );
        }
        // The ids are Altium's TIeeeSymbol, gaps and all.
        assert_eq!(PinSymbol::from_id(8), PinSymbol::PostponedOutput);
        assert_eq!(PinSymbol::from_id(17), PinSymbol::ActiveLowOutput);
        assert_eq!(PinSymbol::from_id(22), PinSymbol::OpenCollectorPullUp);
        assert_eq!(PinSymbol::from_id(34), PinSymbol::BidirectionalSignalFlow);
        // An id past the table is kept, not reset to no decoration.
        assert_eq!(PinSymbol::from_id(35), PinSymbol::Other(35));
        assert_eq!(PinSymbol::None.to_id(), 0);
    }
}
