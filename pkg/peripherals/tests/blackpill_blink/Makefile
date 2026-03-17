CC = arm-none-eabi-gcc
OBJCOPY = arm-none-eabi-objcopy

ARCH = -mcpu=cortex-m4 -mthumb
CFLAGS = $(ARCH) -O1 -Wall -Wextra -ffreestanding -nostdlib
LDFLAGS = -T flash.ld -Wl,-Map=firmware.map

all: firmware.bin

firmware.elf: main.c flash.ld
	$(CC) $(CFLAGS) $(LDFLAGS) -o $@ main.c

firmware.bin: firmware.elf
	$(OBJCOPY) -O binary $< $@

clean:
	rm -f firmware.elf firmware.bin firmware.map

.PHONY: all clean
