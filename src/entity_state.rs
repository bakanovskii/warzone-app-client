//! Original EntityObject scalar state, independent of guest memory and layout.
//! Float values are transported as raw IEEE-754 words: no arithmetic, clamping,
//! NaN canonicalization or conversion is part of these original accessors.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Field {
    Type,
    Direction,
    X,
    Y,
    State,
    BuildStep,
    BuildStartTime,
    BuildTotalTime,
    ProductionType,
    ProductionStartTime,
    IsDeleted,
    IsInBattle,
    ContainingPlaymoneyPercentage,
    ContainingFuelPercentage,
    ContainingIronPercentage,
    ContainingPlaymoney,
    ContainingFuel,
    ContainingIron,
}

pub const FIELDS: [Field; 18] = [
    Field::Type,
    Field::Direction,
    Field::X,
    Field::Y,
    Field::State,
    Field::BuildStep,
    Field::BuildStartTime,
    Field::BuildTotalTime,
    Field::ProductionType,
    Field::ProductionStartTime,
    Field::IsDeleted,
    Field::IsInBattle,
    Field::ContainingPlaymoneyPercentage,
    Field::ContainingFuelPercentage,
    Field::ContainingIronPercentage,
    Field::ContainingPlaymoney,
    Field::ContainingFuel,
    Field::ContainingIron,
];

impl Field {
    pub fn name(self) -> &'static str {
        match self {
            Self::Type => "type",
            Self::Direction => "direction",
            Self::X => "x",
            Self::Y => "y",
            Self::State => "state",
            Self::BuildStep => "buildStep",
            Self::BuildStartTime => "buildStartTime",
            Self::BuildTotalTime => "buildTotalTime",
            Self::ProductionType => "productionType",
            Self::ProductionStartTime => "productionStartTime",
            Self::IsDeleted => "isDeleted",
            Self::IsInBattle => "isInBattle",
            Self::ContainingPlaymoneyPercentage => "containingPlaymoneyPercentage",
            Self::ContainingFuelPercentage => "containingFuelPercentage",
            Self::ContainingIronPercentage => "containingIronPercentage",
            Self::ContainingPlaymoney => "containingPlaymoney",
            Self::ContainingFuel => "containingFuel",
            Self::ContainingIron => "containingIron",
        }
    }
    pub fn is_signed_byte(self) -> bool {
        matches!(self, Self::IsDeleted | Self::IsInBattle)
    }
}

/// Original `ldrb; sxtb`, not Objective-C BOOL normalization to zero/one.
pub fn field_value(field: Field, stored: u32) -> u32 {
    if field.is_signed_byte() {
        (stored as u8 as i8 as i32) as u32
    } else {
        stored
    }
}

/// Original `strb` truncation for char properties; full bits for int/float.
pub fn field_storage(field: Field, argument: u32) -> u32 {
    if field.is_signed_byte() {
        argument & 0xff
    } else {
        argument
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EntityScalars {
    words: [u32; 18],
}
impl EntityScalars {
    pub fn from_words(words: [u32; 18]) -> Self {
        Self {
            words: std::array::from_fn(|i| field_storage(FIELDS[i], words[i])),
        }
    }
    pub fn read(&self, field: Field) -> u32 {
        field_value(field, self.words[field as usize])
    }
    pub fn write(&mut self, field: Field, argument: u32) {
        self.words[field as usize] = field_storage(field, argument);
    }
    pub fn raw_words(&self) -> &[u32; 18] {
        &self.words
    }
}

/// Exact equality cases in -getDirection. Combined/unknown direction bits use
/// the original default "se"; this is not a bitmask membership test.
pub fn direction_name(direction: i32) -> &'static str {
    match direction {
        0x20_0000 => "sw",
        0x40_0000 => "ne",
        0x80_0000 => "nw",
        _ => "se",
    }
}
