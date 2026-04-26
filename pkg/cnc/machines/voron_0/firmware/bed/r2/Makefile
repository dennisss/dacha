TARGET = bedboard_stm32c0

BUILD_DIR = build

C_SOURCES = \
main.c \
../../../../../../../third_party/stm/cmsis_device_c0/Source/Templates/system_stm32c0xx.c

ASM_SOURCES = \
../../../../../../../third_party/stm/cmsis_device_c0/Source/Templates/gcc/startup_stm32c011xx.s

PREFIX = arm-none-eabi-
CC = $(PREFIX)gcc
AS = $(PREFIX)gcc -x assembler-with-cpp
CP = $(PREFIX)objcopy
SZ = $(PREFIX)size
HEX = $(CP) -O ihex
BIN = $(CP) -O binary -S

CPU = -mcpu=cortex-m0plus
MCU = $(CPU) -mthumb

C_DEFS = \
-DSTM32C011xx

C_INCLUDES = \
-I. \
-I../../../../../../../third_party/stm/cmsis_device_c0/Include \
-I../../../../../../../third_party/stm/cmsis_core/Include

OPT = -Os -flto

CFLAGS = $(MCU) $(C_DEFS) $(C_INCLUDES) $(OPT) -Wall -fdata-sections -ffunction-sections

ifeq ($(DEBUG), 1)
CFLAGS += -g -gdwarf-2
endif

LDSCRIPT = stm32c011_flash.ld

LDFLAGS = $(MCU) -specs=nano.specs -T$(LDSCRIPT) -Wl,-Map=$(BUILD_DIR)/$(TARGET).map,--cref -Wl,--gc-sections

all: $(BUILD_DIR)/$(TARGET).elf $(BUILD_DIR)/$(TARGET).hex $(BUILD_DIR)/$(TARGET).bin

OBJECTS = $(addprefix $(BUILD_DIR)/,$(notdir $(C_SOURCES:.c=.o)))
vpath %.c $(sort $(dir $(C_SOURCES)))
OBJECTS += $(addprefix $(BUILD_DIR)/,$(notdir $(ASM_SOURCES:.s=.o)))
vpath %.s $(sort $(dir $(ASM_SOURCES)))

$(BUILD_DIR)/%.o: %.c Makefile | $(BUILD_DIR) 
	@echo "CC $<"
	@$(CC) -c $(CFLAGS) -Wa,-a,-ad,-alms=$(BUILD_DIR)/$(notdir $(<:.c=.lst)) $< -o $@

$(BUILD_DIR)/%.o: %.s Makefile | $(BUILD_DIR)
	@echo "AS $<"
	@$(AS) -c $(CFLAGS) $< -o $@

$(BUILD_DIR)/$(TARGET).elf: $(OBJECTS) Makefile
	@echo "LD $@"
	@$(CC) $(OBJECTS) $(LDFLAGS) -o $@
	@$(SZ) $@

$(BUILD_DIR)/%.hex: $(BUILD_DIR)/%.elf | $(BUILD_DIR)
	@echo "HEX $@"
	@$(HEX) $< $@

$(BUILD_DIR)/%.bin: $(BUILD_DIR)/%.elf | $(BUILD_DIR)
	@echo "BIN $@"
	@$(BIN) $< $@

$(BUILD_DIR):
	mkdir $@

clean:
	-rm -fR $(BUILD_DIR)

flash: $(BUILD_DIR)/$(TARGET).elf
	~/apps/gcc-arm-none-eabi-10.3-2021.10/bin/arm-none-eabi-gdb -nx --batch \
	  -ex 'target extended-remote /dev/ttyACM0' \
	  -ex 'monitor swdp_scan' \
	  -ex 'attach 1' \
	  -ex 'load' \
	  -ex 'compare-sections' \
	  -ex 'kill' \
	  $<

.PHONY: all clean flash
