TARGET = pps_divider

# Platform Selection (Default: stm32f411)
PLATFORM ?= stm32f411

# Tools
CC = arm-none-eabi-gcc
OBJCOPY = arm-none-eabi-objcopy
SIZE = arm-none-eabi-size

# Directories
WORKSPACE_DIR = ../../../../
SRC_DIR = src
INC_DIR = inc
STARTUP_DIR = startup
BUILD_DIR = build
LD_DIR = ld

# Source Files
# Common Sources
SRCS = $(SRC_DIR)/main.c
SRCS += $(SRC_DIR)/protocol.c
SRCS += $(SRC_DIR)/pll.c
SRCS += $(SRC_DIR)/crc.c

# Unified Linker Script
LDSCRIPT = $(LD_DIR)/stm32.ld

# Platform Specific
ifeq ($(PLATFORM),stm32f411)
    SRCS += $(wildcard $(SRC_DIR)/platform/stm32f411/*.c)
    SRCS += $(STARTUP_DIR)/startup_stm32f411.c
    
    CFLAGS_PLAT = -mcpu=cortex-m4 -mthumb -mfloat-abi=hard -mfpu=fpv4-sp-d16
    CFLAGS_PLAT += -DSTM32F411xE
else ifeq ($(PLATFORM),stm32g031)
    SRCS += $(wildcard $(SRC_DIR)/platform/stm32g031/*.c)
    SRCS += $(STARTUP_DIR)/startup_stm32g031.c
    
    CFLAGS_PLAT = -mcpu=cortex-m0plus -mthumb
    CFLAGS_PLAT += -DSTM32G031xx
else
    $(error Invalid PLATFORM specified)
endif

OBJS = $(SRCS:%.c=$(BUILD_DIR)/$(PLATFORM)/%.o)

# Flags
CFLAGS = $(CFLAGS_PLAT)
CFLAGS += -g -Wall -Wextra -std=c99
CFLAGS += -I$(INC_DIR)
CFLAGS += -I$(WORKSPACE_DIR)/third_party/stm/cmsis_core/Include
CFLAGS += -I$(WORKSPACE_DIR)/third_party/stm/cmsis_device_f4/Include
CFLAGS += -I$(WORKSPACE_DIR)/third_party/stm/cmsis_device_g0/Include

CFLAGS += -O2

LDFLAGS = $(CFLAGS_PLAT)
LDFLAGS += -T$(LDSCRIPT)
LDFLAGS += -Wl,--gc-sections
LDFLAGS += -specs=nano.specs -specs=nosys.specs
LDFLAGS += -lc -lm -lnosys

# Build Rules
all: $(BUILD_DIR)/$(PLATFORM)/$(TARGET).elf $(BUILD_DIR)/$(PLATFORM)/$(TARGET).bin

$(BUILD_DIR)/$(PLATFORM)/$(TARGET).elf: $(OBJS)
	@mkdir -p $(@D)
	$(CC) $(LDFLAGS) -o $@ $^
	$(SIZE) $@

$(BUILD_DIR)/$(PLATFORM)/$(TARGET).bin: $(BUILD_DIR)/$(PLATFORM)/$(TARGET).elf
	$(OBJCOPY) -O binary $< $@

$(BUILD_DIR)/$(PLATFORM)/%.o: %.c
	@mkdir -p $(@D)
	$(CC) $(CFLAGS) -c $< -o $@

clean:
	rm -rf $(BUILD_DIR)

flash: $(BUILD_DIR)/$(PLATFORM)/$(TARGET).bin
	dfu-util -d 0483:df11 -a 0 -s 0x08000000:leave -D $<

.PHONY: all clean flash
