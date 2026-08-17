#[macro_use]
extern crate macros;
#[macro_use]
extern crate common;

use std::sync::Arc;
use std::collections::HashSet;
use std::time::{Duration, Instant};

use base_args::define_arg_command;
use base_util::InRange;
use base_error::*;
use file::LocalPathBuf;
use peripherals_service::mcp23008::*;
use peripherals_service::device::PeripheralsDevice;
use scpi::*;
use cluster_jbod::management::*;

/*

cargo run --bin jbod_tester -- test-management

cargo run --bin jbod_tester -- test-leds

Expected backplane resistance:
- N-Channel: ~30mOhm
- P-Channel: ~60mOhm
- Test Wires (0.6m 18AWG): ~13mOhm

*/

#[derive(Args)]
struct Args {
    mode: Mode,
}

define_arg_command!(Mode {
    TestBackplaneCommand = "test-backplane",
    TestBootTimeCommand = "test-boot-time",
    TestPowerCommand = "test-power",
    TestManagementCommand = "test-management",
    TestLEDCommand = "test-leds",
});

macro_rules! define_backplane_tester_pins {
    ($( $name:ident ( $mcp_pin:expr , $mcp_ctrl_pin:expr, $analog_periph:expr ) ),* $(,)?) => {

        #[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
        pub enum BackplaneTesterPin {
            $($name,)*
        }

        impl BackplaneTesterPin {
            pub fn all() -> impl Iterator<Item = Self> {
                [ $( Self::$name, )* ].iter().cloned()
            }
            
            pub fn mcp_pin(&self) -> Option<usize> {
                match self {
                    $(
                        Self::$name => $mcp_pin,
                    )*
                }
            }

            pub fn mcp_ctrl_pin(&self) -> Option<usize> {
                match self {
                    $(
                        Self::$name => $mcp_ctrl_pin,
                    )*
                }
            }

            pub fn analog_periph(&self) -> Option<&'static str> {
                match self {
                    $(
                        Self::$name => $analog_periph,
                    )*
                }
            }
        }

    }
}

define_backplane_tester_pins!(
    InV1(Some(6), Some(7), None),
    InV2(Some(2), Some(3), None),
    InGnd1(Some(4), Some(5), None),
    InGnd2(Some(0), Some(1), None),

    SasV1(None, None, Some("sas_v1_sense")),
    SasV2(None, None, Some("sas_v2_sense")),
    SasGnd1(None, None, Some("sas_gnd1_sense")),
    SasGnd2(None, None, Some("sas_gnd2_sense")),
);

// TODO: Eventually get rid of these.
const BOARD_IN_GND2: usize = 0;
const BOARD_IN_GND2_CTRL: usize = 1;
const BOARD_IN_V2: usize = 2;
const BOARD_IN_V2_CTRL: usize = 3;
const BOARD_IN_GND1: usize = 4;
const BOARD_IN_GND1_CTRL: usize = 5;
const BOARD_IN_V1: usize = 6;
const BOARD_IN_V1_CTRL: usize = 7;

struct BackplaneTester {
    device: Arc<PeripheralsDevice>,
    mcp: MCP23008
}

impl BackplaneTester {
    async fn create() -> Result<Self> {
        let mut configs = peripherals_service::config::BoardConfigRegistry::defaults().await?;

        let config = configs.remove(&"jbod_backplane_tester")
            .ok_or_else(|| err_msg("No config with the given name"))?;

        let (mut device, _) = PeripheralsDevice::create(&config).await?;

        let device = Arc::new(device);

        let mcp = MCP23008::create(device.clone(), "mcp23008_i2c");

        Ok(Self {
            device,
            mcp
        })
    }

    async fn read_continuity_levels(&self) -> Result<HashSet<BackplaneTesterPin>> {

        let mut out = HashSet::default();

        let mcp_levels = self.mcp.read().await?;

        for pin in [
            BackplaneTesterPin::InV1,
            BackplaneTesterPin::InV2,
            BackplaneTesterPin::InGnd1,
            BackplaneTesterPin::InGnd2,
        ] {
            let level = mcp_levels.get(pin.mcp_pin().unwrap());
            if !level {
                out.insert(pin);
            }
        }

        Ok(out)
    } 

    async fn read_output_continuity_levels(&self) -> Result<HashSet<BackplaneTesterPin>> {
        let mut out = HashSet::default();

        for pin in [
            BackplaneTesterPin::SasV1,
            BackplaneTesterPin::SasV2,
            BackplaneTesterPin::SasGnd1,
            BackplaneTesterPin::SasGnd2,
        ] {
            let v = self.device.analog_read(pin.analog_periph().unwrap()).await?;
            if v > 4.7 {
                //
            } else if v < 0.1 {
                out.insert(pin);
            } else {
                println!("V @ {:?}: {}", pin, v);
                return Err(err_msg("Output pin voltage in undefined region"));
            }
        }

        Ok(out)
    }

}

#[derive(Args)]
struct TestBackplaneCommand {
    #[arg(default = false)]
    full_self_test: bool,

    log_path: Option<LocalPathBuf>,

    board_id: Option<String>,
}

impl TestBackplaneCommand {
    async fn run(self) -> Result<()> {

        if let Some(path) = &self.log_path {
            if !self.board_id.is_some() {
                return Err(err_msg("Log path specified without a board id"));
            }

            if !file::exists(path).await? {
                file::write(path, "board_id,sas_port,res1,res2\n").await?;
            }
        }

        const INPUT_PINS: [BackplaneTesterPin; 4] = [
            BackplaneTesterPin::InV1,
            BackplaneTesterPin::InV2,
            BackplaneTesterPin::InGnd1,
            BackplaneTesterPin::InGnd2,
        ];

        const OUTPUT_PINS: [BackplaneTesterPin; 4] = [
            BackplaneTesterPin::SasV1,
            BackplaneTesterPin::SasV2,
            BackplaneTesterPin::SasGnd1,
            BackplaneTesterPin::SasGnd2,
        ];

        let tester = BackplaneTester::create().await?;

        println!("Please verify that nothing is plugged in. Continue: [y/N]");
        if !file::read_user_confirmation().await? {
            return Ok(());
        }
        println!("");

        {
            println!("[Self Test: 5V Rail Present]");
            let v5_sense = tester.device.analog_read("5v_sense").await?;
            println!("=> V: {}", v5_sense);

            if v5_sense < 4.8 || v5_sense > 5.4 {
                return Err(err_msg("No 5V input into the testing board"));
            }
        }

        tester.mcp.set_levels(&[
            // Only used when we switch these to outputs.
            (BOARD_IN_V1, false),
            (BOARD_IN_V2, false),
            (BOARD_IN_GND1, false),
            (BOARD_IN_GND2, false),

            (BOARD_IN_V1_CTRL, true),
            (BOARD_IN_V2_CTRL, true),
            (BOARD_IN_GND1_CTRL, false),
            (BOARD_IN_GND2_CTRL, false),
        ]).await?;

        tester.mcp.set_pull_ups(&[
            (BOARD_IN_V1, true),
            (BOARD_IN_V2, true),
            (BOARD_IN_GND1, true),
            (BOARD_IN_GND2, true),

            (BOARD_IN_V1_CTRL, false),
            (BOARD_IN_V2_CTRL, false),
            (BOARD_IN_GND1_CTRL, false),
            (BOARD_IN_GND2_CTRL, false),

        ]).await?;

        tester.mcp.set_directions(&[
            (BOARD_IN_V1, PinDirection::Input),
            (BOARD_IN_V2, PinDirection::Input),
            (BOARD_IN_GND1, PinDirection::Input),
            (BOARD_IN_GND2, PinDirection::Input),

            (BOARD_IN_V1_CTRL, PinDirection::Output),
            (BOARD_IN_V2_CTRL, PinDirection::Output),
            (BOARD_IN_GND1_CTRL, PinDirection::Output),
            (BOARD_IN_GND2_CTRL, PinDirection::Output),
        ]).await?;
        
        // We will drive all input pins to a strong high (5V) level.
        // The expectation is that the SAS output pins will still be
        // low since there is no connections in the test board itself.
        println!("[Self Test: Inputs/outputs isolated]");
        {
            for pin in INPUT_PINS {
                tester.mcp.set_level(pin.mcp_pin().unwrap(), true).await?;
                tester.mcp.set_directions(&[(pin.mcp_pin().unwrap(), PinDirection::Output)]).await?;
            }

            assert_eq!(tester.read_continuity_levels().await?, [].into());

            for pin in OUTPUT_PINS {
                let v = tester.device.analog_read(pin.analog_periph().unwrap()).await?;
                assert!(v < 0.1);
            }

            for pin in INPUT_PINS {
                tester.mcp.set_level(pin.mcp_pin().unwrap(), false).await?
            }

            for pin in OUTPUT_PINS {
                let v = tester.device.analog_read(pin.analog_periph().unwrap()).await?;
                assert!(v < 0.1);
            }

            // Reset to defaults
            for pin in INPUT_PINS {
                tester.mcp.set_directions(&[(pin.mcp_pin().unwrap(), PinDirection::Input)]).await?;
            }

            println!("=> Pass");
        }

        // Drive one input pin low at a time while others are weakly pulled high.
        // We expect just that one pin that will show up as low. 
        println!("[Self Test: Inputs not connected]");
        {
            // Verify initial state is zero state has all input pins are high due to pull up.
            assert_eq!(tester.read_continuity_levels().await?, [].into());

            for pin in INPUT_PINS {
                tester.mcp.set_directions(&[
                    (pin.mcp_pin().unwrap(), PinDirection::Output),
                ]).await?;

                let cont = tester.read_continuity_levels().await?;
                assert_eq!(cont, [pin].into());

                tester.mcp.set_directions(&[
                    (pin.mcp_pin().unwrap(), PinDirection::Input),
                ]).await?;
            }

            println!("=> Pass");
        }

        println!("[Self Test: GND mosfets]");
        {
            for pin in [
                BackplaneTesterPin::InGnd1,
                BackplaneTesterPin::InGnd2,
            ] {
                tester.mcp.set_level(pin.mcp_ctrl_pin().unwrap(), true).await?;

                let cont = tester.read_continuity_levels().await?;
                assert_eq!(cont, [pin].into());

                tester.mcp.set_level(pin.mcp_ctrl_pin().unwrap(), false).await?;
            }

            println!("=> Pass");
        }

        if self.full_self_test {
            println!("[Self Calibration: Resistor Bridge Scaling]");

            for pin in OUTPUT_PINS {
                println!("Please bridge: 5V to {:?}. Continue: [y/N]", pin);
                if !file::read_user_confirmation().await? {
                    return Ok(());
                }

                let v1 = tester.device.analog_read("5v_sense").await?;
                let v2 = tester.device.analog_read(pin.analog_periph().unwrap()).await?;

                println!("{:?} scaling: {} / {} = {}", pin, v2, v1, v2 / v1);
            }
        }

        if self.full_self_test {
            println!("[Self Test: VCC mosfets]");
            for (in_pin, out_pin) in [
                (BackplaneTesterPin::InV1, BackplaneTesterPin::SasV1),
                (BackplaneTesterPin::InV2, BackplaneTesterPin::SasV2)
            ] {
                println!("Please bridge: {:?} and {:?}. Continue: [y/N]", in_pin, out_pin);
                if !file::read_user_confirmation().await? {
                    return Ok(());
                }

                // Drive low though weak resistor.
                tester.mcp.set_directions(&[
                    (in_pin.mcp_pin().unwrap(), PinDirection::Output),
                ]).await?;

                tester.mcp.set_level(in_pin.mcp_pin().unwrap(), true).await?;
                
                let v = tester.device.analog_read(out_pin.analog_periph().unwrap()).await?;
                println!("=> Weak V: {}", v);
                assert!(v > 4.2);

                tester.mcp.set_level(in_pin.mcp_pin().unwrap(), false).await?;

                println!("=> Was bridged");

                let v = tester.device.analog_read(out_pin.analog_periph().unwrap()).await?;
                assert!(v < 0.1);

                // Drive high though strong mosfet
                tester.mcp.set_level(in_pin.mcp_ctrl_pin().unwrap(), false).await?;

                let v = tester.device.analog_read(out_pin.analog_periph().unwrap()).await?;
                assert!(v > 4.8);

                tester.mcp.set_level(in_pin.mcp_ctrl_pin().unwrap(), true).await?;

                let v = tester.device.analog_read(out_pin.analog_periph().unwrap()).await?;
                assert!(v < 0.1);

                // Reset back to original state.
                tester.mcp.set_directions(&[
                    (in_pin.mcp_pin().unwrap(), PinDirection::Input),
                ]).await?;

                println!("=> Pass");
            }
        }

        // NOTE: We don't test the VCC input readers since those are tested as part of the 'VCC mosfets test'
        if self.full_self_test {
            println!("[Self Test: GND output readers]");

            for (in_pin, out_pin) in [
                (BackplaneTesterPin::InGnd1, BackplaneTesterPin::SasGnd1),
                (BackplaneTesterPin::InGnd2, BackplaneTesterPin::SasGnd2)
            ] {
                println!("Please bridge: {:?} and {:?}. Continue: [y/N]", in_pin, out_pin);
                if !file::read_user_confirmation().await? {
                    return Ok(());
                }

                tester.mcp.set_directions(&[
                    (in_pin.mcp_pin().unwrap(), PinDirection::Output),
                ]).await?;
                tester.mcp.set_level(in_pin.mcp_pin().unwrap(), true).await?;

                let v = tester.device.analog_read(out_pin.analog_periph().unwrap()).await?;
                println!("=> Weak V: {}", v);
                assert!(v > 4.2);

                // Reset to defaults.
                tester.mcp.set_directions(&[
                    (in_pin.mcp_pin().unwrap(), PinDirection::Input),
                ]).await?;
                tester.mcp.set_level(in_pin.mcp_pin().unwrap(), false).await?;

                println!("=> Pass");
            }
        }

        assert_eq!(tester.read_continuity_levels().await?, [].into());

        // Before connecting the backplane, all voltages are high due to the weak pullup.

        println!("Please plug power into the backplane. Continue: [y/N]");
        if !file::read_user_confirmation().await? {
            return Ok(());
        }

        println!("[Backplane Test: GNDs connected]");
        {
            // Charge all caps through (1K + 1K resistors).
            tester.mcp.set_levels(&[
                (BackplaneTesterPin::InV1.mcp_pin().unwrap(), true),
                (BackplaneTesterPin::InV2.mcp_pin().unwrap(), true),
                (BackplaneTesterPin::InGnd1.mcp_pin().unwrap(), false),
                (BackplaneTesterPin::InGnd2.mcp_pin().unwrap(), false),
            ]).await?;
            tester.mcp.set_directions(&[
                (BackplaneTesterPin::InV1.mcp_pin().unwrap(), PinDirection::Output),
                (BackplaneTesterPin::InV2.mcp_pin().unwrap(), PinDirection::Output),
                (BackplaneTesterPin::InGnd1.mcp_pin().unwrap(), PinDirection::Output),
                (BackplaneTesterPin::InGnd2.mcp_pin().unwrap(), PinDirection::Output),
            ]).await?;
            executor::sleep(Duration::from_secs(3)).await?;

            // Make all pins other than one of the GNDs inputs (pulled up).
            tester.mcp.set_directions(&[
                (BackplaneTesterPin::InV1.mcp_pin().unwrap(), PinDirection::Input),
                (BackplaneTesterPin::InV2.mcp_pin().unwrap(), PinDirection::Input),
                (BackplaneTesterPin::InGnd1.mcp_pin().unwrap(), PinDirection::Input),
                // (BackplaneTesterPin::InGnd2.mcp_pin().unwrap(), PinDirection::Output),
            ]).await?;

            executor::sleep(Duration::from_secs(1)).await?;

            assert_eq!(tester.read_continuity_levels().await?, [
                BackplaneTesterPin::InGnd1,
                BackplaneTesterPin::InGnd2
            ].into());

            println!("=> Pass");
        }

        /*
        - Set one of the VCCs low and allow the cap to drain
        - Verify other cap is still high.
        */

        println!("[Backplane Test: VCCs not connected]");
        {
            // Set V2 to low so that one set of capacitors drains through it.
            tester.mcp.set_levels(&[
                (BackplaneTesterPin::InV2.mcp_pin().unwrap(), false),
            ]).await?;
            tester.mcp.set_directions(&[
                (BackplaneTesterPin::InV2.mcp_pin().unwrap(), PinDirection::Output),
            ]).await?;

            // Wait for stability.
            executor::sleep(Duration::from_secs(3)).await?;

            // Note: 'V1' should still be high if the two VCCs are isolated since they are separate
            // capacitors.
            assert_eq!(tester.read_continuity_levels().await?, [
                BackplaneTesterPin::InV2,
                BackplaneTesterPin::InGnd1,
                BackplaneTesterPin::InGnd2
            ].into());

            println!("=> Pass");
        }

        for sas_port_i in 0..4 {
            // Set all inputs to High. This will discharge all caps.
            tester.mcp.set_levels(&[
                (BackplaneTesterPin::InV1.mcp_pin().unwrap(), true),
                (BackplaneTesterPin::InV2.mcp_pin().unwrap(), true),
                (BackplaneTesterPin::InGnd1.mcp_pin().unwrap(), true),
                (BackplaneTesterPin::InGnd2.mcp_pin().unwrap(), true),

                (BackplaneTesterPin::InGnd1.mcp_ctrl_pin().unwrap(), false),
                (BackplaneTesterPin::InGnd2.mcp_ctrl_pin().unwrap(), false),

            ]).await?;
            tester.mcp.set_directions(&[
                (BackplaneTesterPin::InV1.mcp_pin().unwrap(), PinDirection::Output),
                (BackplaneTesterPin::InV2.mcp_pin().unwrap(), PinDirection::Output),
                (BackplaneTesterPin::InGnd1.mcp_pin().unwrap(), PinDirection::Output),
                (BackplaneTesterPin::InGnd2.mcp_pin().unwrap(), PinDirection::Output),
            ]).await?;
            executor::sleep(Duration::from_secs(3)).await?;


            println!("Please plug in SAS port #{:?}. Continue: [y/N]", sas_port_i);
            if !file::read_user_confirmation().await? {
                return Ok(());
            }

            println!("[Port #{} Test: Input to output passthrough]", sas_port_i);
            {
                // All SAS outputs should have high voltage.
                assert_eq!(tester.read_output_continuity_levels().await?, [].into());
                println!("=> Pass");
            }

            // Although we have already verified this at the backplane level, we are here
            // verifying that the grounds are connected to the correct pins on the SAS
            // connector.
            println!("[Port #{} Test: GNDs connected]", sas_port_i);
            {
                // Charge all caps through (1K + 1K resistors).
                tester.mcp.set_levels(&[
                    (BackplaneTesterPin::InV1.mcp_pin().unwrap(), true),
                    (BackplaneTesterPin::InV2.mcp_pin().unwrap(), true),
                    (BackplaneTesterPin::InGnd1.mcp_pin().unwrap(), false),
                    (BackplaneTesterPin::InGnd2.mcp_pin().unwrap(), false),

                    // Off
                    (BackplaneTesterPin::InV1.mcp_ctrl_pin().unwrap(), true),
                    (BackplaneTesterPin::InV2.mcp_ctrl_pin().unwrap(), true),
                    // On
                    (BackplaneTesterPin::InGnd1.mcp_ctrl_pin().unwrap(), true),
                    (BackplaneTesterPin::InGnd2.mcp_ctrl_pin().unwrap(), true),

                ]).await?;
                tester.mcp.set_directions(&[
                    (BackplaneTesterPin::InV1.mcp_pin().unwrap(), PinDirection::Output),
                    (BackplaneTesterPin::InV2.mcp_pin().unwrap(), PinDirection::Output),
                    (BackplaneTesterPin::InGnd1.mcp_pin().unwrap(), PinDirection::Output),
                    (BackplaneTesterPin::InGnd2.mcp_pin().unwrap(), PinDirection::Output),
                ]).await?;

                executor::sleep(Duration::from_secs(4)).await?;

                assert_eq!(tester.read_output_continuity_levels().await?, [
                    BackplaneTesterPin::SasGnd1,
                    BackplaneTesterPin::SasGnd2
                ].into());

                println!("=> Pass");
            }

            // Similar to above, the main goal here is to verify that the right voltage
            // pins map correctly between input and outputs. By removing one of the input
            // voltages, we expect the correct pin on the output to also drop.
            println!("[Port #{} Test: VCCs not connected]", sas_port_i);
            {
                // Set V2 to low so that one set of capacitors drains through it.
                tester.mcp.set_levels(&[
                    (BackplaneTesterPin::InV2.mcp_pin().unwrap(), false),
                ]).await?;

                // Wait for stabilization.
                executor::sleep(Duration::from_secs(4)).await?;

                assert_eq!(tester.read_output_continuity_levels().await?, [
                    BackplaneTesterPin::SasV2,
                    BackplaneTesterPin::SasGnd1,
                    BackplaneTesterPin::SasGnd2
                ].into());

                println!("=> Pass");
            }

            // NOTE: Each resistance measurement is done using effectively just one
            // GND pin of the two given to the backplane (other one is relatively high resistance). 
            println!("[Port #{} Test: V1 resistance]", sas_port_i);
            let res1;
            {
                // Full power
                tester.mcp.set_levels(&[
                    (BackplaneTesterPin::InV1.mcp_pin().unwrap(), true),
                    (BackplaneTesterPin::InV2.mcp_pin().unwrap(), true),
                    (BackplaneTesterPin::InGnd1.mcp_pin().unwrap(), false),
                    (BackplaneTesterPin::InGnd2.mcp_pin().unwrap(), false),


                    // First pair on.
                    (BackplaneTesterPin::InV1.mcp_ctrl_pin().unwrap(), false),
                    (BackplaneTesterPin::InGnd1.mcp_ctrl_pin().unwrap(), true),

                    // Second pair off.
                    (BackplaneTesterPin::InV2.mcp_ctrl_pin().unwrap(), true),
                    (BackplaneTesterPin::InGnd2.mcp_ctrl_pin().unwrap(), false),
                ]).await?;

                // Wait for stabilization.
                // NOTE: Charging should be very fast now.
                executor::sleep(Duration::from_secs(1)).await?;

                println!("=> Baseline");

                let v5_sense = tester.device.analog_read("5v_sense").await?;
                println!("  => Vin: {}", v5_sense);

                let v1 = tester.device.analog_read(BackplaneTesterPin::SasV1.analog_periph().unwrap()).await?;
                println!("  => V1: {}", v1);

                let gnd1 = tester.device.analog_read(BackplaneTesterPin::SasGnd1.analog_periph().unwrap()).await?;
                println!("  => GND1: {}", gnd1);

                tester.device.gpio_write("power_res1_ctrl", true).await?;

                // Wait for stabilization.
                // NOTE: Charging should be very fast now.
                executor::sleep(Duration::from_secs(1)).await?;

                println!("=> Under Load");

                let v5_sense = tester.device.analog_read("5v_sense").await?;
                println!("  => Vin: {}", v5_sense);

                let v1 = tester.device.analog_read(BackplaneTesterPin::SasV1.analog_periph().unwrap()).await?;
                println!("  => V1: {}", v1);

                let gnd1 = tester.device.analog_read(BackplaneTesterPin::SasGnd1.analog_periph().unwrap()).await?;
                println!("  => GND1: {}", gnd1);

                tester.device.gpio_write("power_res1_ctrl", false).await?;

                res1 = self.calculate_backplane_resistance(v5_sense, v1, gnd1);
                println!("=> Resistance: {}", res1);

                if res1 > 0.4 {
                    return Err(err_msg("Backplane/SAS resistance too high"));
                }
                // Shouldn't be this low given we should at least have test setup loses.
                if res1 < 0.1 {
                    return Err(err_msg("Backplane/SAS resistance too low"));
                }

                println!("=> Pass");

                // Recovery from the power resistor being turned off before we start the next test.
                executor::sleep(Duration::from_secs(1)).await?;
            }
            
            println!("[Port #{} Test: V2 resistance]", sas_port_i);
            let res2;
            {
                tester.mcp.set_levels(&[
                    // First pair off
                    (BackplaneTesterPin::InV1.mcp_ctrl_pin().unwrap(), true),
                    (BackplaneTesterPin::InGnd1.mcp_ctrl_pin().unwrap(), false),

                    // Second pair on
                    (BackplaneTesterPin::InV2.mcp_ctrl_pin().unwrap(), false),
                    (BackplaneTesterPin::InGnd2.mcp_ctrl_pin().unwrap(), true),
                ]).await?;

                executor::sleep(Duration::from_secs(1)).await?;

                println!("=> Baseline");

                let v5_sense = tester.device.analog_read("5v_sense").await?;
                println!("  => Vin: {}", v5_sense);

                let v2 = tester.device.analog_read(BackplaneTesterPin::SasV2.analog_periph().unwrap()).await?;
                println!("  => V2: {}", v2);

                let gnd2 = tester.device.analog_read(BackplaneTesterPin::SasGnd2.analog_periph().unwrap()).await?;
                println!("  => GND2: {}", gnd2);

                tester.device.gpio_write("power_res2_ctrl", true).await?;

                // Wait for stabilization.
                // NOTE: Charging should be very fast now.
                executor::sleep(Duration::from_secs(1)).await?;

                println!("=> Under Load:");

                let v5_sense = tester.device.analog_read("5v_sense").await?;
                println!("  => Vin: {}", v5_sense);

                let v2 = tester.device.analog_read(BackplaneTesterPin::SasV2.analog_periph().unwrap()).await?;
                println!("  => V2: {}", v2);

                let gnd2 = tester.device.analog_read(BackplaneTesterPin::SasGnd2.analog_periph().unwrap()).await?;
                println!("  => GND2: {}", gnd2);

                tester.device.gpio_write("power_res2_ctrl", false).await?;

                res2 = self.calculate_backplane_resistance(v5_sense, v2, gnd2);
                println!("=> Resistance: {}", res2);

                if res2 > 0.4 {
                    return Err(err_msg("Backplane/SAS resistance too high"));
                }
                if res2 < 0.1 {
                    return Err(err_msg("Backplane/SAS resistance too low"));
                }

                println!("=> Pass");
            }

            // TODO: Add capacitance measuring.

            if let Some(log_path) = &self.log_path {
                let id = self.board_id.as_ref().unwrap();
                file::append(log_path, format!("{},{},{},{}\n", id, sas_port_i, res1, res2)).await?;
            }
        }

        // TODO: Explicitly discharge all the caps at the end.


        Ok(())
    }

    fn calculate_backplane_resistance(&self, vcc: f32, v_load_high: f32, v_load_low: f32) -> f32 {
        let v_load = v_load_high - v_load_low;
        let r_load = 2.7;

        r_load * (vcc - v_load) / v_load
    }
}



#[derive(Args)]
pub struct TestPowerCommand {
    multimeter_addr: String,
}

impl TestPowerCommand {
    pub async fn run(self) -> Result<()> {

        let mut multimeter = SCPIClient::create(&self.multimeter_addr).await?;
        multimeter.check_instrument_type(InstrumentType::Multimeter).await?;

        let mut management_device = ManagementDevice::create().await?;

        let tester_device = {
            let mut configs = peripherals_service::config::BoardConfigRegistry::defaults().await?;

            let config = configs.remove(&"jbod_power_tester")
                .ok_or_else(|| err_msg("No config with the given name"))?;

            let (mut device, _) = PeripheralsDevice::create(&config).await?;

            Arc::new(device)
        };

        management_device.power_on().await?;

        loop {
            println!("Please plug in a power cable [y/N]");
            if !file::read_user_confirmation().await? {
                return Ok(());
            }

            let v1 = multimeter.measure_voltage().await?;
            println!("V1: {}", v1);

            tester_device.gpio_write("relay_coil_ctrl", true).await?;
            executor::sleep(Duration::from_secs(1)).await?;

            let v2 = multimeter.measure_voltage().await?;
            println!("V2: {}", v2);

            tester_device.gpio_write("relay_coil_ctrl", false).await?;
        }
    }
}


#[derive(Args)]
pub struct TestManagementCommand {

}

impl TestManagementCommand {
    pub async fn run(self) -> Result<()> {

        let mut management_device = ManagementDevice::create().await?;

        {
            let mut current = 0;
            let num_leds = 15 * 4;

            loop {

                let mut buf = vec![];
                for i in 0..num_leds {
                    // if current % num_leds == i {
                    //     buf.extend_from_slice(&[0x20, 0x00, 0x00]);
                    // } else {
                    //     buf.extend_from_slice(&[0x00, 0x20, 0x00]);
                    // }

                    buf.extend_from_slice(&[0x00, 0x00, 0x20]);

                    // if i % 3 == 0 {
                            
                    // }
                    // if i % 3 == 1 {
                    //     buf.extend_from_slice(&[0x00, 0x05, 0x00]);    
                    // }
                    // if i % 3 == 2 {
                    //     buf.extend_from_slice(&[0x00, 0x00, 0x05]);    
                    // }
                }


                let red_i = current % num_leds;
                {
                    let j = red_i * 3;
                    buf[j..(j + 3)].copy_from_slice(&[0x00, 0x20, 0x00]);      
                }
                
                let green_j = (num_leds - 1) - (current % num_leds);
                {
                    let j = green_j * 3;
                    buf[j..(j + 3)].copy_from_slice(&[0x20, 0x00, 0x00]);      
                }


                management_device.set_led_data(&buf).await?;
                executor::sleep(Duration::from_millis(200)).await?;

                current += 1;
            }


            // loop {

            // }


        }

        management_device.power_on().await?;

        management_device.set_fan_speed(0.7).await?;


        // println!("Power on SAS...");
        // management_device.power_on_sas().await?;

        let cancellation_token = executor::signals::new_shutdown_token();

        while !cancellation_token.is_cancelled() {
            let speed = management_device.get_fan_speeds().await?;

            println!("Speed: {:?}", speed);

            executor::sleep(Duration::from_secs(1)).await?;
        }

        println!("Powering off");
        management_device.power_off().await?;

        Ok(())
    }
}


#[derive(Args)]
pub struct TestLEDCommand {

}

impl TestLEDCommand {
    pub async fn run(self) -> Result<()> {
        let mut configs = peripherals_service::config::BoardConfigRegistry::defaults().await?;

        let config = configs.remove(&"jbod_led_tester")
            .ok_or_else(|| err_msg("No config with the given name"))?;

        let (mut device, _) = PeripheralsDevice::create(&config).await?;

        let mut buf = vec![];
        for i in 0..(15 * 4) {
            let i = i +1;

            if i % 3 == 0 {
                buf.extend_from_slice(&[0x20, 0x00, 0x00]);    
            }
            if i % 3 == 1 {
                buf.extend_from_slice(&[0x00, 0x20, 0x00]);    
            }
            if i % 3 == 2 {
                buf.extend_from_slice(&[0x00, 0x00, 0x20]);    
            }
        }

        device.neopixel_transfer("led", 0, &buf[..]).await?;

        loop {
            // println!("Show!");
            device.neopixel_show("led").await?;
            executor::sleep(Duration::from_millis(200)).await?;
        }


        Ok(())
    }
}

#[derive(Args)]
pub struct TestBootTimeCommand {

}

impl TestBootTimeCommand {

    pub async fn run(self) -> Result<()> {
        let tester = BackplaneTester::create().await?;

        // Initialize. Power off.
        {
            tester.mcp.set_levels(&[
                // Off
                (BackplaneTesterPin::InV2.mcp_ctrl_pin().unwrap(), true),
                // On
                (BackplaneTesterPin::InGnd2.mcp_ctrl_pin().unwrap(), true),
            ]).await?;
            tester.mcp.set_directions(&[
                (BackplaneTesterPin::InV2.mcp_ctrl_pin().unwrap(), PinDirection::Output),
                (BackplaneTesterPin::InGnd2.mcp_ctrl_pin().unwrap(), PinDirection::Output),
            ]).await?;
        }


        loop {
            println!("Turn on load. Continue: [y/N]");
            if !file::read_user_confirmation().await? {
                return Ok(());
            }

            println!("Powering on load...");
            tester.mcp.set_levels(&[
                (BackplaneTesterPin::InV2.mcp_ctrl_pin().unwrap(), false),
            ]).await?;

            let start_time = Instant::now();

            println!("Waiting for GPIO boot signal...");
            loop {
                let v = tester.device.analog_read(BackplaneTesterPin::SasV2.analog_periph().unwrap()).await?;
                if v > 2.0 {
                    break;
                }
                executor::sleep(Duration::from_millis(50)).await?;
            }

            let end_time = Instant::now();

            println!("Boot Took: {:?}", end_time - start_time);

            println!("Power off load. Continue: [y/N]");
            if !file::read_user_confirmation().await? {
                return Ok(());
            }

            tester.mcp.set_levels(&[
                (BackplaneTesterPin::InV2.mcp_ctrl_pin().unwrap(), true),
            ]).await?;
        }
    }

}



#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;
    args.mode.run().await
}