use std::collections::HashMap;

use base_error::*;
use common::hash::FastHasherBuilder;
use gcode_decimal::Decimal;

use crate::parser::*;
use crate::{define_command, define_command_enum, define_unparsed_command};

/*
Types to support:
- Decimal
- String
- u32 : Strictly no floating point part.
- bool : Presence of field only.
*/

#[derive(Clone, PartialEq)]
pub struct CommandWord {
    pub group: char,
    pub number: Decimal,
}

impl CommandWord {
    pub fn from_word(word: &Word) -> Result<Self> {
        let number = match word.value {
            WordValue::RealValue(v) => v,
            _ => {
                return Err(format_err!(
                    "Command does not have a valid number: {:?}.",
                    word
                ))
            }
        };

        Ok(CommandWord {
            group: word.key,
            number,
        })
    }

    pub fn to_string(&self) -> String {
        format!("{}{}", self.group, self.number)
    }
}

// Defines the 'Command' enum.
define_command_enum!(
    RapidMove,                       // G0
    LinearMove,                      // G1
    ClockwiseArc,                    // G2
    CounterClockwiseArc,             // G3
    Dwell,                           // G4
    PlaneXY,                         // G17
    PlaneZX,                         // G18
    PlaneYZ,                         // G19
    SetUnitsToInches,                // G20
    SetUnitsToMillimeters,           // G21
    MoveToOriginHome,                // G28
    DetailedZProbe,                  // G29
    CutterCompensationOff,           // G40
    Workspace1Coordinates,           // G54
    G80,                             // G80
    SetToAbsoluteMode,               // G90
    SetToRelativeMode,               // G91
    SetPosition,                     // G92
    FeedRateUnitsPerMinute,          // G94
    ProgramEnd,                      // M2
    SpindleOnClockwise,              // M3
    SpindleOnCounterClockwise,       // M4
    SpindleOff,                      // M5
    ToolChange,                      // M6
    MistCoolantOn,                   // M7
    CoolantOff,                      // M9
    EnableSteppers,                  // M17
    DisableSteppers,                 // M18
    ProgramStop,                     // M30
    SetBuildPercentage,              // M73
    SetWeightOnPrintBed,             // M74
    StopPrintJobTimer,               // M77
    SetExtruderToAbsoluteMode,       // M82
    SetExtruderToRelativeMode,       // M83
    StopMotors,                      // M84
    SetAxisStepsPerUnit,             // M92
    SetExtruderTemperature,          // M104
    GetExtruderTemperature,          // M105
    FanOn,                           // M106
    FanOff,                          // M107
    SetExtruderTemperatureAndWait,   // M109
    SetDebugLevel,                   // M111
    GetCurrentPosition,              // M114
    PrintFirmwareCapabilities,       // M115
    GetTachometerValue,              // M123
    SetBedTemperature,               // M140
    SetHeatbreakTargetTemperature,   // M142
    SetupAutoReport,                 // M155
    SetBedTemperatureAndWaitCommand, // M190
    SetMaxAcceleration,              // M201
    SetMaxFeedRate,                  // M203
    SetDefaultAcceleration,          // M204
    AdvancedSettings,                // M205
    ToolchangeParameters,            // M217
    SetExtrudeFactorOverride,        // M221
    AllowColdExtrudes,               // M302
    CarveraEnterLaserMode,           // M321
    CarveraExitLaserMode,            // M322
    WaitForCurrentMovesToFinish,     // M400
    CancelObject,                    // M486
    SetBoundingBox,                  // M555
    StepperDriverControl,            // M569
    ExtruderPressureAdvance,         // M572
    ConfigureInputShaping,           // M593
    NozzleDiameter,                  // M862.1
    PrusaModelName,                  // M862.3
    GcodeLevel,                      // M862.5
    FirmwareFeatures,                // M862.6
    SetLinearAdvanceScalingFactors,  // M900
    SetMotorCurrent,                 // M907
    SelectTool,                      // Tn
    ParkTool                         // Pn
);

define_command!(
    pub struct Move ("-0") {
        x ('X'): Option<Decimal>,
        y ('Y'): Option<Decimal>,
        z ('Z'): Option<Decimal>,
        e ('E'): Option<Decimal>,
        feed_rate ('F'): Option<Decimal>
    }
);

define_command!(
    pub struct RapidMove ("G0") {
        inner ('-'): Move
    }
);

define_command!(
    pub struct LinearMove ("G1") {
        inner ('-'): Move
    }
);

define_command!(
    pub struct ArcMove ("-0") {
        x ('X'): Option<Decimal>,
        y ('Y'): Option<Decimal>,
        z ('Z'): Option<Decimal>,
        i ('I'): Option<Decimal>,
        j ('J'): Option<Decimal>,
        k ('K'): Option<Decimal>,
        e ('E'): Option<Decimal>,
        feed_rate ('F'): Option<Decimal>
    }
);

define_command!(
    pub struct ClockwiseArc ("G2") {
        inner ('-'): ArcMove
    }
);

define_command!(
    pub struct CounterClockwiseArc ("G3") {
        inner ('-'): ArcMove
    }
);

define_command!(
    pub struct Dwell ("G4") {}
);

define_command!(
    pub struct PlaneXY ("G17") {}
);

define_command!(
    pub struct PlaneZX ("G18") {}
);

define_command!(
    pub struct PlaneYZ ("G19") {}
);

define_command!(
    pub struct SetUnitsToInches ("G20") {}
);

define_command!(
    pub struct SetUnitsToMillimeters ("G21") {}
);

define_command!(
    pub struct MoveToOriginHome ("G28") {
        x ('X'): bool,
        y ('Y'): bool,
        z ('Z'): bool,
        w ('W'): bool
    }
);

define_unparsed_command!(
    pub struct DetailedZProbe ("G29")
);

define_command!(
    pub struct CutterCompensationOff ("G40") {}
);

define_command!(
    pub struct Workspace1Coordinates ("G54") {}
);

define_unparsed_command!(
    pub struct G80 ("G80")
);

define_command!(
    pub struct SetToAbsoluteMode ("G90") {}
);

define_command!(
    pub struct SetToRelativeMode ("G91") {}
);

// TODO: Verify during parsing and serialization that at least one parameter
// word is present to avoid ambiguous behavior.
define_command!(
    pub struct SetPosition ("G92") {
        x ('X'): Option<Decimal>,
        y ('Y'): Option<Decimal>,
        z ('Z'): Option<Decimal>,
        e ('E'): Option<Decimal>
    }
);

define_command!(
    pub struct FeedRateUnitsPerMinute ("G94") {}
);

define_command!(
    pub struct ProgramEnd ("M2") {}
);

define_command!(
    pub struct SpindleOnClockwise ("M3") {
        speed ('S'): Option<i32>
    }
);

define_command!(
    pub struct SpindleOnCounterClockwise ("M4") {
        speed ('S'): Option<i32>
    }
);

define_command!(
    pub struct SpindleOff ("M5") {}
);

define_command!(
    pub struct ToolChange ("M6") {
        tool ('T'): i32
    }
);

define_command!(
    pub struct MistCoolantOn ("M7") {}
);

define_command!(
    pub struct CoolantOff ("M9") {}
);

define_command!(
    pub struct EnableSteppers ("M17") {}
);

define_command!(
    pub struct DisableSteppers ("M18") {}
);

define_command!(
    /// TODO: This means 'delete an sdcard file' on most firmwares
    pub struct ProgramStop ("M30") {}
);

define_command!(
    pub struct SetBuildPercentage ("M73") {
        normal_percentage ('P'): Option<Decimal>,
        normal_time_remaining_mins ('R'): Option<Decimal>,
        silent_percentage ('Q'): Option<Decimal>,
        silent_time_remaining_mins ('S'): Option<Decimal>
    }
);

define_command!(
    pub struct SetWeightOnPrintBed ("M74") {
        weight ('W'): Decimal
    }
);

define_command!(
    pub struct StopPrintJobTimer ("M77") {}
);

define_command!(
    pub struct SetExtruderToAbsoluteMode ("M82") {}
);

define_command!(
    pub struct SetExtruderToRelativeMode ("M83") {}
);

define_command!(
    pub struct StopMotors ("M84") {
        e ('E'): bool
    }
);

define_unparsed_command!(
    pub struct SetAxisStepsPerUnit ("M92")
);

define_command!(
    pub struct SetHeaterTemperature ("-0") {
        /// When setting the extruder temperature on a multi-tool machine, indicates which on the tools' heaters to change.
        tool ('T'): Option<i32>,

        /// Target temperature to achieve.
        /// While waiting, the actual temperature must be >= this value.
        min_temperature ('S'): Option<Decimal>,

        /// Target temperature for heating/cooling.
        ///
        /// ONLY allowed for the '*AndWait' commands.
        ///
        /// When waiting, actual temperature must heat/cool to hit 'exactly' this temperature.
        target_temperature ('R'): Option<Decimal>
    }
);

define_command!(
    pub struct SetExtruderTemperature ("M104") {
        inner ('-'): SetHeaterTemperature
    }
);

define_command!(
    pub struct GetExtruderTemperature ("M105") {}
);

define_command!(
    pub struct FanOn ("M106") {
        speed ('S'): Option<Decimal>
    }
);

define_command!(
    pub struct FanOff ("M107") {}
);

define_command!(
    pub struct SetExtruderTemperatureAndWait ("M109") {
        inner ('-'): SetHeaterTemperature
    }
);

define_command!(
    pub struct SetDebugLevel ("M111") {
        s ('S'): Decimal
    }
);

define_command!(
    pub struct GetCurrentPosition ("M114") {}
);

define_unparsed_command!(
    pub struct PrintFirmwareCapabilities ("M115")
);

define_command!(
    pub struct GetTachometerValue ("M123") {}
);

define_command!(
    pub struct SetBedTemperature ("M140") {
        inner ('-'): SetHeaterTemperature
    }
);

define_command!(
    pub struct SetHeatbreakTargetTemperature ("M142") {
        temperature ('S'): Decimal
    }
);

define_command!(
    pub struct SetupAutoReport ("M155") {
        interval_secs ('S'): i32,
        flags ('C'): Option<i32>
    }
);

define_command!(
    pub struct SetBedTemperatureAndWaitCommand ("M190") {
        inner ('-'): SetHeaterTemperature
    }
);

define_unparsed_command!(
    pub struct SetMaxAcceleration ("M201")
);

define_unparsed_command!(
    pub struct SetMaxFeedRate ("M203")
);

define_unparsed_command!(
    pub struct SetDefaultAcceleration ("M204")
);

define_unparsed_command!(
    pub struct AdvancedSettings ("M205")
);

define_unparsed_command!(
    pub struct ToolchangeParameters ("M217")
);

define_unparsed_command!(
    pub struct SetExtrudeFactorOverride ("M221")
);

define_unparsed_command!(
    pub struct AllowColdExtrudes ("M302")
);

define_unparsed_command!(
    pub struct CarveraEnterLaserMode ("M321")
);

define_unparsed_command!(
    pub struct CarveraExitLaserMode ("M322")
);

define_command!(
    pub struct WaitForCurrentMovesToFinish ("M400") {}
);

define_command!(
    pub struct CancelObject ("M486") {
        total_num_objects ('T'): Option<i32>,
        starting_object_index ('S'): Option<i32>,
        // MUST BE THE LAST PARAMETER
        object_name ('A'): Option<String>
    }
);

define_command!(
    pub struct SetBoundingBox ("M555") {
        x ('X'): Decimal,
        y ('Y'): Decimal,
        w ('W'): Decimal,
        h ('H'): Decimal
    }
);

define_unparsed_command!(
    pub struct StepperDriverControl ("M569")
);

define_unparsed_command!(
    pub struct ExtruderPressureAdvance ("M572")
);

define_unparsed_command!(
    pub struct ConfigureInputShaping ("M593")
);

define_command!(
    pub struct NozzleDiameter ("M862.1") {
        tool ('T'): Option<i32>,
        diameter ('P'): Decimal
    }
);

define_command!(
    pub struct PrusaModelName ("M862.3") {
        model_name ('P'): String
    }
);

define_unparsed_command!(
    pub struct GcodeLevel ("M862.5")
);

define_unparsed_command!(
    pub struct FirmwareFeatures ("M862.6")
);

define_unparsed_command!(
    pub struct SetLinearAdvanceScalingFactors ("M900")
);

define_unparsed_command!(
    pub struct SetMotorCurrent ("M907")
);

#[derive(Clone, Debug)]
pub struct SelectTool {
    pub index: i32,
    pub params: Vec<Word>,
}

impl SelectTool {
    // NOTE: This is never used since the command is dynamic.
    const COMMAND: CommandWord = CommandWord {
        group: '-',
        number: Decimal::from_raw(0),
    };
}

impl CommandCodec for SelectTool {
    fn from_command_words(command: CommandWord, params: &mut LineParameters) -> Result<Self> {
        if command.group != 'T' {
            return Err(err_msg("Wrong command group"));
        }

        if command.number.to_f32().floor() != command.number.to_f32() {
            return Err(err_msg("Expected an integer tool number"));
        }

        let index = command.number.to_f32() as i32;
        let params = params
            .take_all()?
            .into_iter()
            .filter(|w| w.key != 'T')
            .collect();

        Ok(Self { index, params })
    }

    fn to_command_words(&self, out: &mut Vec<Word>) {
        out.push(Word {
            key: 'T',
            value: self.index.to_word_value().unwrap(),
        });
        for param in &self.params {
            out.push(param.clone());
        }
    }
}

#[derive(Clone, Debug)]
pub struct ParkTool {
    pub index: i32,
    pub params: Vec<Word>,
}

impl ParkTool {
    // NOTE: This is never used since the command is dynamic.
    const COMMAND: CommandWord = CommandWord {
        group: '-',
        number: Decimal::from_raw(0),
    };
}

impl CommandCodec for ParkTool {
    fn from_command_words(command: CommandWord, params: &mut LineParameters) -> Result<Self> {
        if command.group != 'P' {
            return Err(err_msg("Wrong command group"));
        }

        if command.number.to_f32().floor() != command.number.to_f32() {
            return Err(err_msg("Expected an integer tool number"));
        }

        let index = command.number.to_f32() as i32;
        let params = params
            .take_all()?
            .into_iter()
            .filter(|w| w.key != 'P')
            .collect();

        Ok(Self { index, params })
    }

    fn to_command_words(&self, out: &mut Vec<Word>) {
        out.push(Word {
            key: 'P',
            value: self.index.to_word_value().unwrap(),
        });
        for param in &self.params {
            out.push(param.clone());
        }
    }
}

/// Trait defined by all *Command structs for implementing
/// serialization/deserialization to/from raw gcode words.
pub trait CommandCodec {
    /// Decodes a command from parsed gcode words.
    ///
    /// - 'command' will be the G/M/T code (e.g. 'M0')
    /// - 'params' will contain all remaining unparsed parameters in the current
    ///   line. This function should pull out any parameters that could be used
    ///   for this command.
    fn from_command_words(command: CommandWord, params: &mut LineParameters) -> Result<Self>
    where
        Self: Sized;

    fn to_command_words(&self, out: &mut Vec<Word>);
}

// pub trait PartialCommandCodec {
//     fn from_partial_words(params: &mut LineParameters) -> Result<Self>
//     where
//         Self: Sized;

//     fn to_partial_words(&self, out: &mut Vec<Word>);
// }

/// Similar to a HashMap<char, T> which only supports keys which are capital
/// ascii alphabetic letters.
#[derive(Default)]
struct CapitalLetterMap<T> {
    // 65 - 90
    bins: [Option<T>; 26],
}

impl<T> CapitalLetterMap<T> {
    pub fn clear(&mut self) {
        for bin in &mut self.bins {
            bin.take();
        }
    }

    pub fn get(&self, key: &char) -> Option<&T> {
        let mut i = *key as usize;
        if i < (b'A' as usize) {
            return None;
        }

        i -= (b'A' as usize);

        let bin = match self.bins.get(i) {
            Some(v) => v,
            None => return None,
        };

        bin.as_ref()
    }

    pub fn get_mut(&mut self, key: &char) -> Option<&mut T> {
        let mut i = *key as usize;
        if i < (b'A' as usize) {
            return None;
        }

        i -= (b'A' as usize);

        let bin = match self.bins.get_mut(i) {
            Some(v) => v,
            None => return None,
        };

        bin.as_mut()
    }

    pub fn contains_key(&self, key: &char) -> bool {
        self.get(key).is_some()
    }

    /// NOTE: This will crash on out of bounds keys.
    pub fn insert(&mut self, key: char, value: T) {
        let mut i = (key as usize) - (b'A' as usize);
        self.bins[i] = Some(value);
    }

    pub fn values(&self) -> impl Iterator<Item = &T> {
        self.bins.iter().filter_map(|v| v.as_ref())
    }
}

/// Set of available command parameters discovered while parsing a line of
/// gcode. Parameters are incrementally taken out of this set while assembling
/// command structs.
#[derive(Default)]
pub struct LineParameters {
    // Keys that contain None were previously retrieved with 'take_param' while parsing the current
    // line.
    params: CapitalLetterMap<Option<WordValue>>,
    order: Vec<char>,
}

impl LineParameters {
    pub fn clear(&mut self) {
        self.params.clear();
        self.order.clear();
    }

    pub fn take_param(&mut self, key: char) -> Result<Option<WordValue>> {
        match self.params.get_mut(&key) {
            Some(v) => match v.take() {
                Some(v) => Ok(Some(v)),
                None => Err(format_err!(
                    "Parameter {} used across multiple commands in the same line",
                    key
                )),
            },
            None => Ok(None),
        }
    }

    pub fn add_param(&mut self, key: char, value: WordValue) -> Result<()> {
        if self.params.contains_key(&key) {
            return Err(format_err!("Duplicate parameter key: {}", key));
        }

        self.params.insert(key, Some(value));
        self.order.push(key);

        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        for v in self.params.values() {
            if v.is_some() {
                return false;
            }
        }

        true
    }

    pub fn take_all(&mut self) -> Result<Vec<Word>> {
        let mut out = vec![];

        for key in self.order.clone() {
            out.push(Word {
                key,
                value: self
                    .take_param(key)?
                    .ok_or_else(|| err_msg("Missing value?"))?,
            });
        }

        Ok(out)
    }

    /// DON'T use for command parsing.
    pub fn peek_has_remaining(&self, key: char) -> bool {
        match self.params.get(&key) {
            Some(Some(_)) => true,
            _ => false,
        }
    }

    pub fn debug_remaining_unparsed(&self) -> Vec<Word> {
        let mut out = vec![];

        for key in &self.order {
            if let Some(Some(value)) = self.params.get(key) {
                out.push(Word {
                    key: *key,
                    value: value.clone(),
                });
            }
        }

        out
    }
}

pub trait FromWords {
    fn from_param_words(key: char, params: &mut LineParameters) -> Result<Self>
    where
        Self: Sized;
}

impl<T: FromWordValue> FromWords for T {
    fn from_param_words(key: char, params: &mut LineParameters) -> Result<Self> {
        let word_value = params.take_param(key)?;
        T::from_word_value(word_value)
    }
}

pub trait FromWordValue {
    // A None value will be passed if the word doesn't exist on the line being
    // parsed.
    fn from_word_value(word_value: Option<WordValue>) -> Result<Self>
    where
        Self: Sized;
}

pub trait ToWords {
    fn to_param_words(&self, key: char, out: &mut Vec<Word>);
}

impl<T: ToWordValue> ToWords for T {
    fn to_param_words(&self, key: char, out: &mut Vec<Word>) {
        if let Some(value) = self.to_word_value() {
            out.push(Word { key, value });
        }
    }
}

pub trait ToWordValue {
    // Should return None if no word should be included.
    fn to_word_value(&self) -> Option<WordValue>;
}

impl<T: FromWordValue> FromWordValue for Option<T> {
    fn from_word_value(word_value: Option<WordValue>) -> Result<Self> {
        if word_value.is_none() {
            return Ok(None);
        }

        Ok(Some(T::from_word_value(word_value)?))
    }
}

impl<T: ToWordValue> ToWordValue for Option<T> {
    fn to_word_value(&self) -> Option<WordValue> {
        match self {
            Some(v) => v.to_word_value(),
            None => None,
        }
    }
}

impl FromWordValue for Decimal {
    fn from_word_value(word_value: Option<WordValue>) -> Result<Self> {
        match word_value {
            Some(WordValue::RealValue(v)) => Ok(v),
            _ => Err(err_msg("Expected decimal parameter")),
        }
    }
}

impl ToWordValue for Decimal {
    fn to_word_value(&self) -> Option<WordValue> {
        Some(WordValue::RealValue(self.clone()))
    }
}

impl FromWordValue for bool {
    fn from_word_value(word_value: Option<WordValue>) -> Result<Self> {
        match word_value {
            Some(WordValue::Empty) => Ok(true),
            None => Ok(false),
            _ => Err(err_msg("If present, this parameter must have no value")),
        }
    }
}

impl ToWordValue for bool {
    fn to_word_value(&self) -> Option<WordValue> {
        if *self {
            Some(WordValue::Empty)
        } else {
            None
        }
    }
}

impl FromWordValue for String {
    fn from_word_value(word_value: Option<WordValue>) -> Result<Self> {
        match word_value {
            Some(WordValue::QuotedString(v)) | Some(WordValue::UnquotedString(v)) => Ok(v),
            _ => Err(err_msg("Unexpected parameter to be a string")),
        }
    }
}

impl ToWordValue for String {
    fn to_word_value(&self) -> Option<WordValue> {
        Some(WordValue::QuotedString(self.clone()))
    }
}

impl FromWordValue for i32 {
    fn from_word_value(word_value: Option<WordValue>) -> Result<Self>
    where
        Self: Sized,
    {
        let v = Decimal::from_word_value(word_value)?;
        if v.to_f32().floor() != v.to_f32() {
            return Err(err_msg("Expected parameter to be an integer"));
        }

        Ok(v.to_f32() as i32)
    }
}

impl ToWordValue for i32 {
    fn to_word_value(&self) -> Option<WordValue> {
        Some(WordValue::RealValue(Decimal::from_i32(*self)))
    }
}
