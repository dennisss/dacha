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
