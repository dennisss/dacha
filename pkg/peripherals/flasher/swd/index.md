# Bit-Banged SWD Programmer

Example usage for flashing a chip attached via GPIO pins to a Raspberry Pi:

```
cargo run --bin builder -- build //pkg/peripherals/flasher/swd:flasher_swd --config=//pkg/builder/config:rpi64
scp -r -i ~/.ssh/id_cluster built/pkg/peripherals/flasher/swd/flasher_swd cluster-user@10.1.1.3:~/

./flasher_swd --clk_pin=27 --io_pin=17 --reset_pin=26 --firmware_path=firmware.bin --target=STM32G031
```


## References

- https://developer.arm.com/documentation/ihi0031/a/Debug-Port-Registers/Debug-Port--DP--register-descriptions/The-Identification-Code-Register--IDCODE
- https://infocenter.nordicsemi.com/index.jsp?topic=%2Fstruct_nrf52%2Fstruct%2Fnrf52832_ps.html
- https://developer.arm.com/documentation/100893/1-0/Debug-and-trace-interface/Serial-Wire-Debug-signals
- https://www.kernelpicnic.net/2018/12/29/Messing-with-SWD-Part-I.html
- https://qcentlabs.com/posts/swd_banger/

## OpenOCD Pi Reference

You can also flash via OpenOCD on Raspberry Pis. This requires compiling OpenOCD at head though:

(`linuxgpiod` must show up when running `openocd --command 'adapter list'`)

```
sudo apt update
sudo apt install git autoconf libtool make pkg-config libgpiod-dev libjim-dev

# Clone OpenOCD
git clone https://github.com/openocd-org/openocd.git
cd openocd
git submodule update --init --recursive

./bootstrap

./configure --enable-linuxgpiod

# Compile and install
make -j4
sudo make install

```

Then you can flash with something like the following:

```
cat <<EOF > openocd_flash.cfg

    adapter driver linuxgpiod

    # RP1 entry from running gpiodetect
    linuxgpiod_gpiochip 0

    adapter gpio swclk 27
    adapter gpio swdio 22
    adapter gpio srst 17

    transport select swd

    source [find target/stm32g0x.cfg]

    # Hold the reset pin low (active) during programming
    reset_config srst_only srst_nogate connect_assert_srst

    init
    reset halt

    program firmware.elf verify reset
    
    exit
EOF

openocd -f openocd_flash.cfg
```

