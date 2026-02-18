Usage Pages:
- 132: x84: 'Power Device Page'
- 133: x85: 'Battery System Page'


One top level 'Power Device Page' Application Collection
- PowerSummary: Physical Collection
- Input: Physical Collection
- Output: Physical Collection

PowerDevice(UPS), 
  PowerDevice(PowerSummary), 
    PowerDevice(iProduct), 
    => FEAT 1  : 1 x 8
    PowerDevice(iSerialNumber), 
    => FEAT 2  : 1 x 8
    UnknownPage { page: ff01, usage: d0 }, 
    => FEAT 27  : 1 x 8
    BatterySystem(Unknown(89)), 
    => FEAT 3  : 1 x 8
    BatterySystem(Unknown(8f)), 
    => FEAT 4  : 1 x 8
    BatterySystem(Unknown(8b)), 
    => FEAT 5  : 1 x 8
    BatterySystem(Unknown(2c)), 
    => FEAT 6  : 1 x 8
    BatterySystem(DesignCapacity), BatterySystem(Unknown(8d)), BatterySystem(Unknown(8e)), BatterySystem(Unknown(8c)), BatterySystem(RemainingCapacityLimit), BatterySystem(Unknown(67)), 
    => FEAT 7  : 6 x 8
    BatterySystem(Unknown(66)), 
    => IN 8  : 1 x 8
    BatterySystem(Unknown(66)), 
    => FEAT 8  : 1 x 8
    BatterySystem(Unknown(68)), 
    => IN 8  : 1 x 16
    BatterySystem(Unknown(68)), 
    => FEAT 8  : 1 x 16
    BatterySystem(Unknown(2a)), 
    => IN 8  : 1 x 16
    BatterySystem(Unknown(2a)), 
    => FEAT 8  : 1 x 16
    PowerDevice(ConfigVoltage), 
    => FEAT 9  : 1 x 8
    PowerDevice(Voltage), 
    => FEAT 10  : 1 x 8
    PowerDevice(PresentStatus), 
      BatterySystem(ACPresent), BatterySystem(Charging), BatterySystem(Discharging), BatterySystem(BelowRemainingCapacityLimit), BatterySystem(FullyCharged), BatterySystem(RemainingTimeLimitExpired), 
      => IN 11  : 6 x 1
      BatterySystem(ACPresent), BatterySystem(Charging), BatterySystem(Discharging), BatterySystem(BelowRemainingCapacityLimit), BatterySystem(FullyCharged), BatterySystem(RemainingTimeLimitExpired), 
      => FEAT 11  : 6 x 1
Unknown: Report {
    state: ItemStateTable {
        locals: {},
        globals: {
            ReportId: 11,
            ReportSize: 2,
            LogicalMax: 1,
            ReportCount: 1,
            LogicalMin: 0,
            UsagePage: 133,
            Unit: 0,
            UnitExponent: 0,
        },
    },
    var: Input(
        constant | array | absolute | no_wrap | linear | preferred_state | no_null_pos | non_volatile | bit_field,
    ),
}
Unknown: Report {
    state: ItemStateTable {
        locals: {},
        globals: {
            ReportId: 11,
            ReportSize: 2,
            LogicalMax: 1,
            ReportCount: 1,
            LogicalMin: 0,
            UsagePage: 133,
            Unit: 0,
            UnitExponent: 0,
        },
    },
    var: Feature(
        constant | array | absolute | no_wrap | linear | preferred_state | no_null_pos | non_volatile | bit_field,
    ),
}
    PowerDevice(AudibleAlarmControl), 
    => FEAT 12  : 1 x 8
    PowerDevice(AudibleAlarmControl), 
    => IN 12  : 1 x 8
    PowerDevice(iManufacturer), 
    => FEAT 13  : 1 x 8
  PowerDevice(Input), 
    PowerDevice(ConfigVoltage), 
    => FEAT 14  : 1 x 8
    PowerDevice(Voltage), 
    => FEAT 15  : 1 x 16
    PowerDevice(LowVoltageTransfer), 
    => FEAT 16  : 1 x 16
    PowerDevice(LowVoltageTransfer), 
    => IN 16  : 1 x 16
    PowerDevice(HighVoltageTransfer), 
    => FEAT 16  : 1 x 16
    PowerDevice(HighVoltageTransfer), 
    => IN 16  : 1 x 16
  PowerDevice(Output), 
    PowerDevice(Voltage), 
    => FEAT 18  : 1 x 16
    PowerDevice(PercentLoad), 
    => FEAT 19  : 1 x 8
    PowerDevice(Test), 
    => FEAT 20  : 1 x 8
    PowerDevice(Test), 
    => IN 20  : 1 x 8
    PowerDevice(DelayBeforeShutdown), 
    => FEAT 21  : 1 x 16
    PowerDevice(DelayBeforeStartup), 
    => FEAT 22  : 1 x 16
    PowerDevice(Boost), 
    => FEAT 23  : 1 x 1
    PowerDevice(Overload), 
    => FEAT 23  : 1 x 1
Unknown: Report {
    state: ItemStateTable {
        locals: {},
        globals: {
            ReportId: 23,
            ReportSize: 6,
            PhysicalMax: 0,
            LogicalMax: 1,
            ReportCount: 1,
            LogicalMin: 0,
            UsagePage: 132,
            Unit: 0,
            PhysicalMin: 0,
            UnitExponent: 0,
        },
    },
    var: Feature(
        constant | array | absolute | no_wrap | linear | preferred_state | no_null_pos | non_volatile | bit_field,
    ),
}
    PowerDevice(ConfigActivePower), 
    => FEAT 24  : 1 x 16
    UnknownPage { page: ff01, usage: 43 }, 
    => FEAT 26  : 1 x 8
    UnknownPage { page: ff01, usage: 43 }, 
    => IN 26  : 1 x 8

