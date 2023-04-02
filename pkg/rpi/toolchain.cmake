set(CMAKE_SYSTEM_NAME "Linux")
set(CMAKE_SYSTEM_PROCESSOR "aarch64")

set(CMAKE_SYSROOT "/opt/dacha/pi/rootfs")
set(CMAKE_FIND_ROOT_PATH "/opt/dacha/pi/rootfs")
set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_PACKAGE ONLY)

set(CMAKE_C_COMPILER "/usr/bin/aarch64-linux-gnu-gcc")
set(CMAKE_ASM_COMPILER "/usr/bin/aarch64-linux-gnu-gcc")
set(CMAKE_CXX_COMPILER "/usr/bin/aarch64-linux-gnu-g++")
set(CMAKE_AR "/usr/bin/aarch64-linux-gnu-ar")
set(CMAKE_LINKER "/usr/bin/aarch64-linux-gnu-ld")
set(CMAKE_NM "/usr/bin/aarch64-linux-gnu-nm")
set(CMAKE_OBJCOPY "/usr/bin/aarch64-linux-gnu-objcopy")
set(CMAKE_OBJDUMP "/usr/bin/aarch64-linux-gnu-objdump")
set(CMAKE_RANLIB "/usr/bin/aarch64-linux-gnu-ranlib")
set(CMAKE_STRIP "/usr/bin/aarch64-linux-gnu-strip")

# '{CMAKE_FIND_ROOT_PATH}/lib/{CMAKE_LIBRARY_ARCHITECTURE}/' will be searched for libraries
# by find_library().
set(CMAKE_LIBRARY_ARCHITECTURE "aarch64-linux-gnu")
