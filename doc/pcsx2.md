docker pull i386/ubuntu:20.04



sudo apt install cmake g++-10-multilib \
	libwxgtk3.0-gtk3-dev:i386 libgtk-3-dev:i386 \
	libaio-dev:i386 libasound2-dev:i386 liblzma-dev:i386 \
	libsdl2-dev:i386 libsoundtouch-dev:i386 \
	libxml2-dev:i386 libpcap0.8-dev:i386

sudo apt install git libjpeg-dev libx11-xcb-dev

sudo update-alternatives --install /usr/bin/gcc gcc /usr/bin/gcc-10 10
sudo update-alternatives --install /usr/bin/g++ g++ /usr/bin/g++-10 10
sudo update-alternatives --install /usr/bin/cc  cc  /usr/bin/gcc 30
sudo update-alternatives --install /usr/bin/c++ c++ /usr/bin/g++ 30

git clone https://github.com/PCSX2/pcsx2.git
git submodule update --init
cd pcsx2 && mkdir build && cd build

cmake -DCMAKE_TOOLCHAIN_FILE=cmake/linux-compiler-i386-multilib.cmake -DCMAKE_BUILD_TYPE=Release -DBUILD_REPLAY_LOADERS=TRUE -DCMAKE_BUILD_PO=FALSE -DGTK3_API=TRUE ..

make -j10
make install
cd ../bin



----


-- The C compiler identification is GNU 10.2.0
-- The CXX compiler identification is GNU 10.2.0
-- Check for working C compiler: /usr/bin/cc
-- Check for working C compiler: /usr/bin/cc -- works
-- Detecting C compiler ABI info
-- Detecting C compiler ABI info - done
-- Detecting C compile features
-- Detecting C compile features - done
-- Check for working CXX compiler: /usr/bin/c++
-- Check for working CXX compiler: /usr/bin/c++ -- works
-- Detecting CXX compiler ABI info
-- Detecting CXX compiler ABI info - done
-- Detecting CXX compile features
-- Detecting CXX compile features - done
-- Building with GNU GCC
-- Cross compilation is enabled.
-- Compiling a i386 build on a x86_64 host.
CMake Warning at cmake/BuildParameters.cmake:442 (message):
  GTK3 is highly experimental besides it requires a wxWidget built with
  __WXGTK3__ support !!!
Call Stack (most recent call first):
  CMakeLists.txt:28 (include)


-- Found ALSA: /usr/lib/i386-linux-gnu/libasound.so (found version "1.2.2") 
-- Found PCAP: /usr/lib/i386-linux-gnu/libpcap.so  
-- Performing Test PCAP_LINKS_SOLO
-- Performing Test PCAP_LINKS_SOLO - Success
-- Looking for pcap_get_pfring_id
-- Looking for pcap_get_pfring_id - not found
-- Found LibXml2: /usr/lib/i386-linux-gnu/libxml2.so (found version "2.9.10") 
-- Found Freetype: /usr/lib/i386-linux-gnu/libfreetype.so (found version "2.10.1") 
-- Could NOT find Gettext (missing: GETTEXT_MSGMERGE_EXECUTABLE GETTEXT_MSGFMT_EXECUTABLE) 
-- Found Git: /usr/bin/git (found version "2.25.1") 
-- Looking for lzma_auto_decoder in /usr/lib/i386-linux-gnu/liblzma.so
-- Looking for lzma_auto_decoder in /usr/lib/i386-linux-gnu/liblzma.so - found
-- Looking for lzma_easy_encoder in /usr/lib/i386-linux-gnu/liblzma.so
-- Looking for lzma_easy_encoder in /usr/lib/i386-linux-gnu/liblzma.so - found
-- Looking for lzma_lzma_preset in /usr/lib/i386-linux-gnu/liblzma.so
-- Looking for lzma_lzma_preset in /usr/lib/i386-linux-gnu/liblzma.so - found
-- Found LibLZMA: /usr/lib/i386-linux-gnu/liblzma.so (found version "5.2.4") 
-- Found OpenGL: /usr/lib/i386-linux-gnu/libOpenGL.so   
-- Found ZLIB: /usr/lib/i386-linux-gnu/libz.so (found version "1.2.11") 
-- Found PNG: /usr/lib/i386-linux-gnu/libpng.so (found version "1.6.37") 
-- Could NOT find Vtune (missing: VTUNE_LIBRARIES VTUNE_INCLUDE_DIRS) 
-- Found wxWidgets: -L/usr/lib/i386-linux-gnu;-pthread;;;-lwx_baseu-3.0;-lwx_gtk3u_core-3.0;-lwx_gtk3u_adv-3.0 (found version "3.0.4") 
-- Found Libc: /usr/lib/i386-linux-gnu/librt.so;/usr/lib/i386-linux-gnu/libdl.so;/usr/lib/i386-linux-gnu/libm.so  
-- Found PkgConfig: /usr/bin/pkg-config (found version "0.29.1") 
-- EGL found
-- X11_XCB not found
-- AIO found
-- LIBUDEV found
-- PORTAUDIO not found
-- SOUNDTOUCH found
-- SDL2 found
-- Found X11: /usr/include   
-- Looking for XOpenDisplay in /usr/lib/i386-linux-gnu/libX11.so;/usr/lib/i386-linux-gnu/libXext.so
-- Looking for XOpenDisplay in /usr/lib/i386-linux-gnu/libX11.so;/usr/lib/i386-linux-gnu/libXext.so - found
-- Looking for gethostbyname
-- Looking for gethostbyname - found
-- Looking for connect
-- Looking for connect - found
-- Looking for remove
-- Looking for remove - found
-- Looking for shmat
-- Looking for shmat - found
-- Looking for IceConnectionNumber in ICE
-- Looking for IceConnectionNumber in ICE - found
-- Found GTK3_GTK: /usr/lib/i386-linux-gnu/libgtk-3.so  
-- Found the following HarfBuzz libraries:
--  HarfBuzz (required): /usr/lib/i386-linux-gnu/libharfbuzz.so
-- Found HarfBuzz: /usr/include/harfbuzz (found version "2.6.4") 

-- Skip build of GSdx: missing dependencies:check these libraries -> opengl, png (>=1.2), zlib (>=1.2.4), X11, liblzma




-- Version: 7.0.3
-- Build type: Release
-- CXX_STANDARD: 11
-- Performing Test has_std_11_flag
-- Performing Test has_std_11_flag - Success
-- Performing Test has_std_0x_flag
-- Performing Test has_std_0x_flag - Success
-- Performing Test SUPPORTS_USER_DEFINED_LITERALS
-- Performing Test SUPPORTS_USER_DEFINED_LITERALS - Success
-- Performing Test FMT_HAS_VARIANT
-- Performing Test FMT_HAS_VARIANT - Success
-- Required features: cxx_variadic_templates
-- Looking for strtod_l
-- Looking for strtod_l - not found
Using precompiled headers.
-- Found PythonInterp: /usr/bin/python3.8 (found version "3.8.5") 
-- Looking for pthread.h
-- Looking for pthread.h - found
-- Performing Test CMAKE_HAVE_LIBC_PTHREAD
-- Performing Test CMAKE_HAVE_LIBC_PTHREAD - Success
-- Found Threads: TRUE  
-- Configuring done
-- Generating done
CMake Warning:
  Manually-specified variables were not used by the project:

    CMAKE_BUILD_PO


-- Build files have been written to: /root/pcsx2/build
root@2c5180550717:~/pcsx2/build# apt install libx11-dev
Reading package lists... Done
Building dependency tree       
Reading state information... Done
libx11-dev is already the newest version (2:1.6.9-2ubuntu1.1).
libx11-dev set to manually installed.
0 upgraded, 0 newly installed, 0 to remove and 57 not upgraded.