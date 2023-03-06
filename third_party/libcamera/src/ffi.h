// This C++ library links to the libcamera headers used by CXX for bindings
// generate and also defines additional wrappers for functions that CXX can't
// handle.

#pragma once

#include <memory>

#include "libcamera/libcamera.h"
#include "rust/cxx.h"
#include "wrapper.h"

namespace libcamera {
// Declare ahead of time as it is referenced in ffi.rs.h
class RequestCompleteSlot;

// For nested C++ types that were exposed to Rust as non-nested symbols, we add
// an alias to the non-nested type name so that CXX doesn't complain.
//
// NOTE: Keep in sync with ExternType definitions in ffi.rs.
using RequestStatus = Request::Status;
using RequestReuseFlag = libcamera::Request::ReuseFlag;
using FrameStatus = libcamera::FrameMetadata::Status;
using FramePlaneMetadata = libcamera::FrameMetadata::Plane;
using CameraConfigurationStatus = libcamera::CameraConfiguration::Status;
}  // namespace libcamera

// Rust types and generated code. Must be after the pure C++ imports.
#include "libcamera/src/ffi.rs.h"

namespace libcamera {

////////////////////////////////////////////////////////////////////////////////
// CameraManager
////////////////////////////////////////////////////////////////////////////////

inline std::unique_ptr<CameraManager> new_camera_manager() {
  return std::make_unique<CameraManager>();
}

rust::Vec<CameraPtr> list_cameras(const CameraManager &camera_manager);

////////////////////////////////////////////////////////////////////////////////
// Camera
////////////////////////////////////////////////////////////////////////////////

std::unique_ptr<CameraConfiguration> generate_camera_configuration(
    Camera &camera, rust::Slice<const StreamRole> stream_roles);

rust::Vec<::StreamPtr> camera_streams(const Camera &camera);

inline bool camera_contains_stream(const Camera &camera, Stream *stream) {
  return camera.streams().contains(stream);
}

class RequestCompleteSlot {
 public:
  RequestCompleteSlot(
      Signal<Request *> *signal,
      rust::Fn<void(const RequestCompleteContext &, const Request &)> handler,
      rust::Box<::RequestCompleteContext> context)
      : signal_(signal),
        handler_(std::move(handler)),
        context_(std::move(context)) {
    signal_->connect(this, &RequestCompleteSlot::signaled);
  }

  ~RequestCompleteSlot() {
    signal_->disconnect(this, &RequestCompleteSlot::signaled);
  }

 private:
  void signaled(Request *request) { (*handler_)(*context_, *request); }

  Signal<Request *> *signal_;
  rust::Fn<void(const RequestCompleteContext &, const Request &)> handler_;
  rust::Box<::RequestCompleteContext> context_;
};

std::unique_ptr<RequestCompleteSlot> camera_connect_request_completed(
    Camera &camera,
    rust::Fn<void(const RequestCompleteContext &, const Request &)> handler,
    rust::Box<::RequestCompleteContext> context);

////////////////////////////////////////////////////////////////////////////////
// StreamConfiguration
////////////////////////////////////////////////////////////////////////////////

rust::String stream_config_to_string(const StreamConfiguration &config);

inline PixelFormat stream_config_pixel_format(
    const StreamConfiguration &config) {
  return config.pixelFormat;
}
inline void stream_config_set_pixel_format(StreamConfiguration &config,
                                           PixelFormat value) {
  config.pixelFormat = value;
}

inline Size stream_config_size(const StreamConfiguration &config) {
  return config.size;
}
inline void stream_config_set_size(StreamConfiguration &config, Size value) {
  config.size = value;
}

inline unsigned int stream_config_stride(const StreamConfiguration &config) {
  return config.stride;
}
inline void stream_config_set_stride(StreamConfiguration &config,
                                     unsigned int value) {
  config.stride = value;
}

inline unsigned int stream_config_frame_size(
    const StreamConfiguration &config) {
  return config.frameSize;
}
inline void stream_config_set_frame_size(StreamConfiguration &config,
                                         unsigned int value) {
  config.frameSize = value;
}

inline unsigned int stream_config_buffer_count(
    const StreamConfiguration &config) {
  return config.bufferCount;
}
inline void stream_config_set_buffer_count(StreamConfiguration &config,
                                           unsigned int value) {
  config.bufferCount = value;
}

inline bool stream_config_has_color_space(const StreamConfiguration &config) {
  return config.colorSpace.has_value();
}
inline ColorSpace stream_config_color_space(const StreamConfiguration &config) {
  return config.colorSpace.value();
}
inline void stream_config_set_color_space(StreamConfiguration &config,
                                          ColorSpace value) {
  config.colorSpace = value;
}
inline void stream_config_clear_color_space(StreamConfiguration &config) {
  config.colorSpace.reset();
}

////////////////////////////////////////////////////////////////////////////////
// PixelFormat
////////////////////////////////////////////////////////////////////////////////

rust::String pixel_format_to_string(const PixelFormat &format);

////////////////////////////////////////////////////////////////////////////////
// StreamFormats
////////////////////////////////////////////////////////////////////////////////

rust::Vec<PixelFormatWrap> stream_formats_pixelformats(
    const StreamFormats &stream_formats);
rust::Vec<SizeWrap> stream_formats_sizes(const StreamFormats &stream_formats,
                                         const PixelFormat &pixelformat);

////////////////////////////////////////////////////////////////////////////////
// ControlValue
////////////////////////////////////////////////////////////////////////////////

inline std::unique_ptr<ControlValue> new_control_value() {
  return std::make_unique<ControlValue>();
}

rust::String control_value_get_string(const ControlValue &value);

void control_value_set_string(ControlValue &value, const rust::String &s);

rust::Vec<rust::String> control_value_get_string_array(
    const ControlValue &value);

template <typename T>
rust::Slice<T> control_value_get_array(const ControlValue &value) {
  auto span = value.get<Span<T>>();
  return rust::Slice(span.data(), span.size());
}

template <typename T>
void control_value_set_array(ControlValue &value, rust::Slice<T> array) {
  value.set(Span<T>(array.data(), array.size()));
}

rust::String control_value_to_string(const ControlValue &value);

////////////////////////////////////////////////////////////////////////////////
// ControlList
////////////////////////////////////////////////////////////////////////////////

inline std::unique_ptr<ControlList> new_control_list() {
  return std::make_unique<ControlList>(controls::controls);
}

rust::Vec<::ControlListEntry> control_list_entries(const ControlList &list);

////////////////////////////////////////////////////////////////////////////////
// ControlInfo
////////////////////////////////////////////////////////////////////////////////

rust::String control_info_to_string(const ControlInfo &info);

////////////////////////////////////////////////////////////////////////////////
// ControlInfoMap
////////////////////////////////////////////////////////////////////////////////

rust::Vec<::ControlInfoMapEntry> control_info_map_entries(
    const ControlInfoMap &map);

////////////////////////////////////////////////////////////////////////////////
// FrameBufferAllocator
////////////////////////////////////////////////////////////////////////////////

inline std::unique_ptr<FrameBufferAllocator> new_frame_buffer_allocator(
    std::shared_ptr<Camera> camera) {
  return std::make_unique<FrameBufferAllocator>(camera);
}

rust::Vec<FrameBufferPtr> get_allocated_frame_buffers(
    const FrameBufferAllocator &allocator, Stream *stream);

////////////////////////////////////////////////////////////////////////////////
// FrameBuffer
////////////////////////////////////////////////////////////////////////////////

rust::Vec<::FrameBufferPlane> frame_buffer_planes(const FrameBuffer &buffer);

::FrameMetadata frame_buffer_metadata(const FrameBuffer &buffer);

////////////////////////////////////////////////////////////////////////////////
// Request
////////////////////////////////////////////////////////////////////////////////

rust::String request_to_string(const Request &request);

}  // namespace libcamera
