#![no_std]
#![feature(asm_experimental_arch, abi_avr_interrupt)]
#![no_main]

/*
Need the latest toolchain from https://www.microchip.com/en-us/tools-resources/develop/microchip-studio/gcc-compilers
- https://ww1.microchip.com/downloads/aemDocuments/documents/DEV/ProductDocuments/SoftwareTools/avr8-gnu-toolchain-3.7.0.1796-linux.any.x86_64.tar.gz

^ THis one is too old

https://packs.download.microchip.com/

Newer builds: https://github.com/ZakKemble/avr-gcc-build/releases


RUSTFLAGS="-C opt-level=s -C panic=abort" cargo build --bin attiny412_blink -Z build-std=core --target pkg/peripherals/avr/targets/attiny412.json


RUSTFLAGS="-C opt-level=s -C panic=abort" cargo build --bin avr -Z build-std=core --target pkg/peripherals/avr/targets/attiny85.json


-----

git clone git://gcc.gnu.org/git/gcc.git



BASE=${BASE:-${CWD}/build/}
PREFIX_GCC_LINUX=${BASE}avr-${NAME_GCC}-x64-linux

confMake "$PREFIX_GCC_LINUX" "$OPTS_BINUTILS"
    ../configure --prefix=$1 $2 $3 --build=`${4:-../config.guess}`
    make -j $JOBCOUNT
    make install-strip

./contrib/download_prerequisites

sudo apt install flex

mkdir build
cd build
../configure \
    --target=avr \
	--enable-languages=c,c++ \
	--disable-nls \
	--disable-libssp \
	--disable-libada \
	--with-dwarf2 \
	--disable-shared \
	--enable-static \
	--enable-mingw-wildcard \
	--enable-plugin \
	--with-gnu-as \
	--with-gnu-ld \
	--without-zstd

~/Downloads/avr8-gnu-toolchain-linux_x86_64/bin/avr-gcc -Os -mmcu=attiny412 -o blink.elf blink.c


Blink pin PA2

===========

Arduino https://github.com/SpenceKonde/megaTinyCore/blob/master/Installation.md

- "Tools > Programmer > SerialUPDI"

*/

use core::{arch::asm, panic::PanicInfo};

#[panic_handler]
fn panic(_panic: &PanicInfo<'_>) -> ! {
    loop {}
}

#[no_mangle]
fn main() {

    unsafe {
        loop {
            asm!("nop");
        }
    }
}