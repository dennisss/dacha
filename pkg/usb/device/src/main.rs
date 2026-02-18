#[macro_use]
extern crate common;
extern crate usb;
#[macro_use]
extern crate macros;

use common::errors::*;
use usb::hid::{GlobalItemTag, LocalItemTag, Report, ReportVariant};

enum_def_with_unknown!(PowerDeviceUsage u32 =>
    Undefined = 0x00,
    iName = 0x01,
    PresentStatus = 0x02,
    ChangedStatus = 0x03,
    UPS = 0x04,
    PowerSupply = 0x05,
    BatterySystem = 0x10,
    BatterySystemID = 0x11,
    Battery = 0x12,
    BatteryID = 0x13,
    Charger = 0x14,
    ChargerID = 0x15,
    PowerConverter = 0x16,
    PowerConverterID = 0x17,
    OutletSystem = 0x18,
    OutletSystemID = 0x19,
    Input = 0x1a,
    InputID = 0x1b,
    Output = 0x1c,
    OutputID = 0x1d,
    Flow = 0x1e,
    FlowID = 0x1f,
    Outlet = 0x20,
    OutletID = 0x21,
    Gang = 0x22,
    GangID = 0x23,
    PowerSummary = 0x24,
    PowerSummaryID = 0x25,
    Voltage = 0x30,
    Current = 0x31,
    Frequency = 0x32,
    ApparentPower = 0x33,
    ActivePower = 0x34,
    PercentLoad = 0x35,
    Temperature = 0x36,
    Humidity = 0x37,
    BadCount = 0x38,
    ConfigVoltage = 0x40,
    ConfigCurrent = 0x41,
    ConfigFrequency = 0x42,
    ConfigApparentPower = 0x43,
    ConfigActivePower = 0x44,
    ConfigPercentLoad = 0x45,
    ConfigTemperature = 0x46,
    ConfigHumidity = 0x47,
    SwitchOnControl = 0x50,
    SwitchOffControl = 0x51,
    ToggleControl = 0x52,
    LowVoltageTransfer = 0x53,
    HighVoltageTransfer = 0x54,
    DelayBeforeReboot = 0x55,
    DelayBeforeStartup = 0x56,
    DelayBeforeShutdown = 0x57,
    Test = 0x58,
    ModuleReset = 0x59,
    AudibleAlarmControl = 0x5a,
    Overload = 0x65,
    Boost = 0x6e,
    Buck = 0x6f,

    iManufacturer = 0xfd,
    iProduct = 0xfe,
    iSerialNumber = 0xff
);

enum_def_with_unknown!(BatterySystemUsage u32 =>
    Undefined = 0,
    SMBBatteryMode = 0x01,
    SMBBatteryStatus = 0x02,
    SMBAlarmWarning = 0x03,
    SMBChargerMode = 0x04,
    SMBChargerStatus = 0x05,
    SMBChargerSpecInfo = 0x06,
    SMBSelectorState = 0x07,
    SMBSelectorPresets = 0x08,
    SMBSelectorInfo = 0x09,

    RemainingCapacityLimit = 0x29,

    BelowRemainingCapacityLimit = 0x42,
    RemainingTimeLimitExpired = 0x43,
    Charging = 0x44,
    Discharging = 0x45,
    FullyCharged = 0x46,
    FullyDischarged = 0x47,
    NeedReplacement = 0x4b,
    AverageCurrent = 0x62,

    DesignCapacity = 0x83,


    ACPresent = 0xd0,
    BatteryPresent = 0xd1
);

#[derive(Debug)]
enum Usage {
    PowerDevice(PowerDeviceUsage),
    BatterySystem(BatterySystemUsage),
    UnknownPage { page: u32, usage: u32 },
}

impl Usage {
    pub fn from(page: u32, value: u32) -> Self {
        if page == 0x84 {
            Usage::PowerDevice(PowerDeviceUsage::from_value(value))
        } else if page == 0x85 {
            Usage::BatterySystem(BatterySystemUsage::from_value(value))
        } else {
            Usage::UnknownPage { page, usage: value }
        }
    }
}

fn visit_report(report: &Report, indent: &str) {
    let usage_page = *report.state.globals.get(&GlobalItemTag::UsagePage).unwrap();

    if !report.state.locals.contains_key(&LocalItemTag::Usage) {
        println!("Unknown: {:#?}", report);
        return;
    }

    print!("{}", indent);
    for usage in report.state.locals.get(&LocalItemTag::Usage).unwrap() {
        let usage = Usage::from(usage_page, *usage);
        print!("{:02x?}, ", usage);
    }
    println!("");

    match &report.var {
        ReportVariant::Collection { typ, children } => {
            let indent = indent.to_string() + "  ";
            for report in children {
                visit_report(report, &indent);
            }
        }
        _ => {
            let report_id = *report.state.globals.get(&GlobalItemTag::ReportId).unwrap();
            let report_size = *report
                .state
                .globals
                .get(&GlobalItemTag::ReportSize)
                .unwrap();
            let report_count = *report
                .state
                .globals
                .get(&GlobalItemTag::ReportCount)
                .unwrap();

            let t = match &report.var {
                ReportVariant::Input(_) => "IN",
                ReportVariant::Output(_) => "OUT",
                ReportVariant::Feature(_) => "FEAT",
                _ => "?",
            };

            println!(
                "{}=> {} {}  : {} x {}",
                indent, t, report_id, report_count, report_size
            );
        }
    }
}

#[executor_main]
async fn main() -> Result<()> {
    let context = usb::Context::create()?;

    let dev = context.open_device(0x0764, 0x0501).await?;

    let hid = usb::hid::HIDDevice::open_with_existing(dev).await?;

    let reports = hid.reports();

    for report in reports {
        visit_report(report, "");
    }

    let mut iproduct = vec![0u8; 1];
    hid.get_report(1, usb::hid::ReportType::Feature, &mut iproduct)
        .await?;

    let mut iserial = vec![0u8; 1];
    hid.get_report(1, usb::hid::ReportType::Feature, &mut iserial)
        .await?;

    let langs = hid.device().read_languages().await?;
    println!("Languages: {:?}", langs);
    println!(
        "Manufacturer: {}",
        hid.device().read_manufacturer_string(langs[0]).await?
    );
    println!(
        "Product: {}",
        hid.device().read_product_string(langs[0]).await?
    );

    println!(
        "HID Product: {}",
        hid.device().read_string(iproduct[0], langs[0]).await?
    );

    {
        let mut voltage = vec![0u8; 1];
        hid.get_report(10, usb::hid::ReportType::Feature, &mut voltage)
            .await?;
        println!("Power Summary -> Voltage: {}", voltage[0]);
    }

    {
        let mut voltage = vec![0u8; 2];
        hid.get_report(15, usb::hid::ReportType::Feature, &mut voltage)
            .await?;
        println!("Input -> Voltage: {:?}", voltage);
    }

    {
        let mut voltage = vec![0u8; 2];
        hid.get_report(18, usb::hid::ReportType::Feature, &mut voltage)
            .await?;
        println!("Output -> Voltage: {:?}", voltage);
    }

    {
        let mut voltage = vec![0u8; 1];
        hid.get_report(19, usb::hid::ReportType::Feature, &mut voltage)
            .await?;
        println!("Output -> Percent Load: {:?}", voltage);
    }

    {
        let mut voltage = [0u8; 2];
        hid.get_report(24, usb::hid::ReportType::Feature, &mut voltage)
            .await?;
        println!(
            "Output -> Config Active Power: {:?}",
            u16::from_le_bytes(voltage)
        );
    }

    for i in 0..100 {
        {
            // bit 0: BatterySystem(ACPresent),
            // bit 1: BatterySystem(Charging),
            // bit 2: BatterySystem(Discharging),
            // bit 3: BatterySystem(BelowRemainingCapacityLimit),
            // bit 4: BatterySystem(FullyCharged)
            // bit 5: BatterySystem(RemainingTimeLimitExpired),

            // Normal:
            //   Power Summary -> Present Status: [17]
            // After Unplugging power:
            //   Power Summary -> Present Status: [4]
            //   ...
            //   Error: DeviceDisconnected

            let mut voltage = [0u8; 1];
            hid.get_report(11, usb::hid::ReportType::Feature, &mut voltage)
                .await?;
            println!("Power Summary -> Present Status: {:?}", voltage);
        }

        executor::sleep(std::time::Duration::from_secs(1)).await;
    }

    Ok(())
}
