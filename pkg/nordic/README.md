

target = "thumbv7em-none-eabihf"

FLASH start at 0
RAM start at 0x20000000


`git clone git@github.com:openocd-org/openocd.git`
- Need the git version as ubuntu has an old version without nrf commands

```
sudo apt install --reinstall ca-certificates
sudo apt install libtool
./bootstrap
./configure
make
sudo make install
```




cargo build --package nordic --target thumbv7em-none-eabihf --release

openocd -f board/nordic_nrf52_dk.cfg -c init -c "reset init" -c halt -c "nrf5 mass_erase" -c "program target/thumbv7em-none-eabihf/release/nordic verify" -c reset -c exit



https://github.com/openocd-org/openocd/blob/master/tcl/board/nordic_nrf52_dk.cfg