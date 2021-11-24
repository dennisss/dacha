

sudo apt install ninja-build python3-yaml python3-ply python3-jinja2 libboost-dev libgnutls28-dev openssl

pip3 install meson
pip3 install --upgrade meson

meson build
ninja -C build install


bindgen third_party/libcamera/build/include/libcamera/libcamera.h -o libcamera.rs -- -Ithird_party/libcamera/build/include

bindgen /usr/local/include/libcamera/libcamera/libcamera.h -o libcamera.rs -- -I/usr/local/include/libcamera

/home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/camera.h


libcamera-dev: /usr/include/libcamera/ipa/ipa_controls.h
libcamera-dev: /usr/include/libcamera/ipa/ipa_interface.h
libcamera-dev: /usr/include/libcamera/ipa/ipa_module_info.h
libcamera-dev: /usr/include/libcamera/libcamera/base/backtrace.h
libcamera-dev: /usr/include/libcamera/libcamera/base/bound_method.h
libcamera-dev: /usr/include/libcamera/libcamera/base/class.h
libcamera-dev: /usr/include/libcamera/libcamera/base/event_dispatcher.h
libcamera-dev: /usr/include/libcamera/libcamera/base/event_dispatcher_poll.h
libcamera-dev: /usr/include/libcamera/libcamera/base/event_notifier.h
libcamera-dev: /usr/include/libcamera/libcamera/base/file.h
libcamera-dev: /usr/include/libcamera/libcamera/base/flags.h
libcamera-dev: /usr/include/libcamera/libcamera/base/log.h
libcamera-dev: /usr/include/libcamera/libcamera/base/message.h
libcamera-dev: /usr/include/libcamera/libcamera/base/object.h
libcamera-dev: /usr/include/libcamera/libcamera/base/private.h
libcamera-dev: /usr/include/libcamera/libcamera/base/semaphore.h
libcamera-dev: /usr/include/libcamera/libcamera/base/signal.h
libcamera-dev: /usr/include/libcamera/libcamera/base/span.h
libcamera-dev: /usr/include/libcamera/libcamera/base/thread.h
libcamera-dev: /usr/include/libcamera/libcamera/base/timer.h
libcamera-dev: /usr/include/libcamera/libcamera/base/utils.h
libcamera-dev: /usr/include/libcamera/libcamera/bound_method.h
libcamera-dev: /usr/include/libcamera/libcamera/buffer.h
libcamera-dev: /usr/include/libcamera/libcamera/camera.h
libcamera-dev: /usr/include/libcamera/libcamera/camera_manager.h
libcamera-dev: /usr/include/libcamera/libcamera/compiler.h
libcamera-dev: /usr/include/libcamera/libcamera/control_ids.h
libcamera-dev: /usr/include/libcamera/libcamera/controls.h
libcamera-dev: /usr/include/libcamera/libcamera/event_dispatcher.h
libcamera-dev: /usr/include/libcamera/libcamera/event_notifier.h
libcamera-dev: /usr/include/libcamera/libcamera/file_descriptor.h
libcamera-dev: /usr/include/libcamera/libcamera/formats.h
libcamera-dev: /usr/include/libcamera/libcamera/framebuffer.h
libcamera-dev: /usr/include/libcamera/libcamera/framebuffer_allocator.h
libcamera-dev: /usr/include/libcamera/libcamera/geometry.h
libcamera-dev: /usr/include/libcamera/libcamera/ipa/core_ipa_interface.h
libcamera-dev: /usr/include/libcamera/libcamera/ipa/ipa_controls.h
libcamera-dev: /usr/include/libcamera/libcamera/ipa/ipa_interface.h
libcamera-dev: /usr/include/libcamera/libcamera/ipa/ipa_module_info.h
libcamera-dev: /usr/include/libcamera/libcamera/ipa/raspberrypi_ipa_interface.h
libcamera-dev: /usr/include/libcamera/libcamera/ipa/vimc_ipa_interface.h
libcamera-dev: /usr/include/libcamera/libcamera/libcamera.h
libcamera-dev: /usr/include/libcamera/libcamera/logging.h
libcamera-dev: /usr/include/libcamera/libcamera/object.h
libcamera-dev: /usr/include/libcamera/libcamera/pixel_format.h
libcamera-dev: /usr/include/libcamera/libcamera/pixelformats.h
libcamera-dev: /usr/include/libcamera/libcamera/property_ids.h
libcamera-dev: /usr/include/libcamera/libcamera/request.h
libcamera-dev: /usr/include/libcamera/libcamera/signal.h
libcamera-dev: /usr/include/libcamera/libcamera/span.h
libcamera-dev: /usr/include/libcamera/libcamera/stream.h
libcamera-dev: /usr/include/libcamera/libcamera/timer.h
libcamera-dev: /usr/include/libcamera/libcamera/transform.h
libcamera-dev: /usr/include/libcamera/libcamera/version.h
libcamera-dev: /usr/lib/arm-linux-gnueabihf/libcamera-base.so
libcamera-dev: /usr/lib/arm-linux-gnueabihf/libcamera.so
libcamera-dev: /usr/lib/arm-linux-gnueabihf/pkgconfig/camera.pc
libcamera-dev: /usr/lib/arm-linux-gnueabihf/pkgconfig/libcamera-base.pc
libcamera-dev: /usr/lib/arm-linux-gnueabihf/pkgconfig/libcamera.pc
libcamera-dev: /usr/lib/arm-linux-gnueabihf/v4l2-compat.so



Installing include/libcamera/ipa/core_ipa_interface.h to /usr/local/include/libcamera/libcamera/ipa
Installing include/libcamera/ipa/ipu3_ipa_interface.h to /usr/local/include/libcamera/libcamera/ipa
Installing include/libcamera/ipa/raspberrypi_ipa_interface.h to /usr/local/include/libcamera/libcamera/ipa
Installing include/libcamera/ipa/rkisp1_ipa_interface.h to /usr/local/include/libcamera/libcamera/ipa
Installing include/libcamera/ipa/vimc_ipa_interface.h to /usr/local/include/libcamera/libcamera/ipa
Installing include/libcamera/control_ids.h to /usr/local/include/libcamera/libcamera
Installing include/libcamera/property_ids.h to /usr/local/include/libcamera/libcamera
Installing include/libcamera/formats.h to /usr/local/include/libcamera/libcamera
Installing include/libcamera/libcamera.h to /usr/local/include/libcamera/libcamera
Installing src/libcamera/base/libcamera-base.so.0.0.0 to /usr/local/lib/x86_64-linux-gnu
Installing src/libcamera/libcamera.so.0.0.0 to /usr/local/lib/x86_64-linux-gnu
Installing src/libcamera/proxy/worker/ipu3_ipa_proxy to /usr/local/libexec/libcamera
Installing src/libcamera/proxy/worker/raspberrypi_ipa_proxy to /usr/local/libexec/libcamera
Installing src/libcamera/proxy/worker/rkisp1_ipa_proxy to /usr/local/libexec/libcamera
Installing src/libcamera/proxy/worker/vimc_ipa_proxy to /usr/local/libexec/libcamera
Installing src/ipa/ipu3/ipa_ipu3.so to /usr/local/lib/x86_64-linux-gnu/libcamera
Installing src/ipa/raspberrypi/ipa_rpi.so to /usr/local/lib/x86_64-linux-gnu/libcamera
Installing src/ipa/rkisp1/ipa_rkisp1.so to /usr/local/lib/x86_64-linux-gnu/libcamera
Installing src/ipa/vimc/ipa_vimc.so to /usr/local/lib/x86_64-linux-gnu/libcamera
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/base/backtrace.h to /usr/local/include/libcamera/libcamera/base
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/base/bound_method.h to /usr/local/include/libcamera/libcamera/base
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/base/class.h to /usr/local/include/libcamera/libcamera/base
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/base/event_dispatcher.h to /usr/local/include/libcamera/libcamera/base
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/base/event_dispatcher_poll.h to /usr/local/include/libcamera/libcamera/base
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/base/event_notifier.h to /usr/local/include/libcamera/libcamera/base
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/base/file.h to /usr/local/include/libcamera/libcamera/base
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/base/flags.h to /usr/local/include/libcamera/libcamera/base
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/base/log.h to /usr/local/include/libcamera/libcamera/base
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/base/message.h to /usr/local/include/libcamera/libcamera/base
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/base/object.h to /usr/local/include/libcamera/libcamera/base
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/base/private.h to /usr/local/include/libcamera/libcamera/base
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/base/semaphore.h to /usr/local/include/libcamera/libcamera/base
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/base/signal.h to /usr/local/include/libcamera/libcamera/base
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/base/span.h to /usr/local/include/libcamera/libcamera/base
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/base/thread.h to /usr/local/include/libcamera/libcamera/base
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/base/timer.h to /usr/local/include/libcamera/libcamera/base
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/base/utils.h to /usr/local/include/libcamera/libcamera/base
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/ipa/ipa_controls.h to /usr/local/include/libcamera/libcamera/ipa
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/ipa/ipa_interface.h to /usr/local/include/libcamera/libcamera/ipa
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/ipa/ipa_module_info.h to /usr/local/include/libcamera/libcamera/ipa
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/camera.h to /usr/local/include/libcamera/libcamera
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/camera_manager.h to /usr/local/include/libcamera/libcamera
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/compiler.h to /usr/local/include/libcamera/libcamera
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/controls.h to /usr/local/include/libcamera/libcamera
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/file_descriptor.h to /usr/local/include/libcamera/libcamera
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/framebuffer.h to /usr/local/include/libcamera/libcamera
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/framebuffer_allocator.h to /usr/local/include/libcamera/libcamera
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/geometry.h to /usr/local/include/libcamera/libcamera
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/logging.h to /usr/local/include/libcamera/libcamera
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/pixel_format.h to /usr/local/include/libcamera/libcamera
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/request.h to /usr/local/include/libcamera/libcamera
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/stream.h to /usr/local/include/libcamera/libcamera
Installing /home/dennis/workspace/dacha/third_party/libcamera/include/libcamera/transform.h to /usr/local/include/libcamera/libcamera
Installing /home/dennis/workspace/dacha/third_party/libcamera/build/include/libcamera/version.h to /usr/local/include/libcamera/libcamera
Installing /home/dennis/workspace/dacha/third_party/libcamera/build/meson-private/libcamera-base.pc to /usr/local/lib/x86_64-linux-gnu/pkgconfig
Installing /home/dennis/workspace/dacha/third_party/libcamera/build/meson-private/libcamera.pc to /usr/local/lib/x86_64-linux-gnu/pkgconfig
Installing /home/dennis/workspace/dacha/third_party/libcamera/src/ipa/raspberrypi/data/imx219.json to /usr/local/share/libcamera/ipa/raspberrypi
Installing /home/dennis/workspace/dacha/third_party/libcamera/src/ipa/raspberrypi/data/imx219_noir.json to /usr/local/share/libcamera/ipa/raspberrypi
Installing /home/dennis/workspace/dacha/third_party/libcamera/src/ipa/raspberrypi/data/imx290.json to /usr/local/share/libcamera/ipa/raspberrypi
Installing /home/dennis/workspace/dacha/third_party/libcamera/src/ipa/raspberrypi/data/imx378.json to /usr/local/share/libcamera/ipa/raspberrypi
Installing /home/dennis/workspace/dacha/third_party/libcamera/src/ipa/raspberrypi/data/imx477.json to /usr/local/share/libcamera/ipa/raspberrypi
Installing /home/dennis/workspace/dacha/third_party/libcamera/src/ipa/raspberrypi/data/imx477_noir.json to /usr/local/share/libcamera/ipa/raspberrypi
Installing /home/dennis/workspace/dacha/third_party/libcamera/src/ipa/raspberrypi/data/imx519.json to /usr/local/share/libcamera/ipa/raspberrypi
Installing /home/dennis/workspace/dacha/third_party/libcamera/src/ipa/raspberrypi/data/ov5647.json to /usr/local/share/libcamera/ipa/raspberrypi
Installing /home/dennis/workspace/dacha/third_party/libcamera/src/ipa/raspberrypi/data/ov5647_noir.json to /usr/local/share/libcamera/ipa/raspberrypi
Installing /home/dennis/workspace/dacha/third_party/libcamera/src/ipa/raspberrypi/data/ov9281.json to /usr/local/share/libcamera/ipa/raspberrypi
Installing /home/dennis/workspace/dacha/third_party/libcamera/src/ipa/raspberrypi/data/se327m12.json to /usr/local/share/libcamera/ipa/raspberrypi
Installing /home/dennis/workspace/dacha/third_party/libcamera/src/ipa/raspberrypi/data/uncalibrated.json to /usr/local/share/libcamera/ipa/raspberrypi

