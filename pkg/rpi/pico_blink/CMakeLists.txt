cmake_minimum_required(VERSION 3.13)

# Initialize the Pico SDK
include(pico_sdk_import.cmake)

project(pico_blink C CXX ASM)

# Initialize the Pico SDK
pico_sdk_init()

# -----------------
# Basic Blink
# -----------------
add_executable(pico_blink main.c)
target_link_libraries(pico_blink pico_stdlib)
pico_add_extra_outputs(pico_blink)

# -----------------
# Low Power Blink
# -----------------
add_executable(pico_blink_low_power main.c)
target_link_libraries(pico_blink_low_power pico_stdlib hardware_powman)
target_compile_definitions(pico_blink_low_power PRIVATE LOW_POWER_MODE=1)
pico_add_extra_outputs(pico_blink_low_power)
