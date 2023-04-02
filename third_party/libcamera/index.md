# Rust bindings for libcamera

The main libcamera repository is also mirrored in the `repo` directory.

To build it:

```bash
sudo apt install ninja-build python3-yaml python3-ply python3-jinja2 libboost-dev libgnutls28-dev openssl

# Only needed for the 'cam' app
sudo apt install libevent-dev

pip3 install meson
pip3 install --upgrade meson

meson build
ninja -C build install
```

When manually installing like this, headers/libraries are installed under `/usr/local/`

## Raspberry Pi

On a Raspberry Pi, headers/libraries are located in the following locations via the `libcamera-dev` package:

```
/usr/include/libcamera/libcamera/camera.h
/usr/include/libcamera/libcamera/camera_manager.h
/usr/lib/arm-linux-gnueabihf/libcamera-base.so
/usr/lib/arm-linux-gnueabihf/libcamera.so
/usr/lib/arm-linux-gnueabihf/pkgconfig/camera.pc
/usr/lib/arm-linux-gnueabihf/pkgconfig/libcamera-base.pc
/usr/lib/arm-linux-gnueabihf/pkgconfig/libcamera.pc
/usr/lib/arm-linux-gnueabihf/v4l2-compat.so
```


## Development

https://rust-lang.github.io/rust-bindgen/requirements.html

CXX dependencies

## Usage

```
sudo usermod -a -G video $USER
```

```
BINDGEN_EXTRA_CLANG_ARGS="-I/usr/local/include/libcamera/" cargo build
```

Download from https://git.libcamera.org/libcamera/libcamera.git/tree/src/libcamera/control_ids.yaml

## Safety/Design

This section contains some notes about how the internal implementation keeps API usage safe:

**Lifetimes**: The raw libcamera APIs require a lot of careful management of memory ownership in order to use
correctly. To avoid exposing this to Rust users, we internally keep dependencies alive through
`Arc` references to them. For example, the `Camera` struct contains an `Arc<CameraManager>` to
ensure that no `Camera`s exist after the `CameraManager` has been shutdown. More specific
implementation notes are provided below.

**Streams**: For a single `libcamera::Camera` instance, we assume that `libcamera::Stream` pointers retrieved from the `Camera` instance will be valid for the entire lifetime of the `Camera`. In other words, streams are never deleted or moved until the camera is deleted.

**Mixing Instances**: The libcamera API may allow a user to mix Request/Stream/FrameBuffer instances from different cameras. We explicitly disallow assert that all associated objects belong to the same Camera so that our lifetimes model can be simplified to mainly maintaining a reference to one Camera instance.

**Control Ids**: We generate `Control<>` instances based on a static list of controls defined in this repository. This list may go out of sync with the one in the main libcamera repository. To ensure some consistency between the C++/Rust code, we re-use the extern ControlId variables exposed by the C++ API. This ensures that minimally the control ids are aligned between the two APIs but can't currently ensure at compile time that types are matching between the C++ and Rust code. So we rely on runtime assertions and the expectation that libcamera doesn't make an incompatible change to the API.

**Bindgen/CXX Usage**: We use bindgen exclusively for exposing trivial Rectangle/Size to Rust so that we they are easier to pass around. All types with non-trivial descructors are opaquelly exposed via CXX.