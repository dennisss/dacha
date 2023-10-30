#pragma once

#include <iostream>
#include <memory>

#include "content_decryption_module.h"
#include "rust/cxx.h"

namespace cdm {
class ContentDecryptionModule;
class DecryptedBlockImpl;
}  // namespace cdm

// Rust types and generated code. Must be after the pure C++ imports.
#include "chromium_cdm/src/ffi.rs.h"

namespace cdm {

class BufferImpl : public Buffer {
 public:
  BufferImpl(uint32_t capacity) {
    data_ = malloc(capacity);
    size_ = capacity;
  }

  void Destroy() override {
    free(data_);
    delete this;
  }

  uint32_t Capacity() const override { return size_; }

  uint8_t* Data() override { return static_cast<uint8_t*>(data_); }

  void SetSize(uint32_t size) override { data_ = realloc(data_, size); }

  uint32_t Size() const override { return size_; }

 private:
  void* data_;
  size_t size_;
};

class DecryptedBlockImpl : public DecryptedBlock {
 public:
  DecryptedBlockImpl() : buffer_(nullptr), timestamp_(0) {}

  ~DecryptedBlockImpl() override {
    if (buffer_ != nullptr) {
      buffer_->Destroy();
    }
  }

  DecryptedBlock* Cast() { return this; }

  void SetDecryptedBuffer(Buffer* buffer) override {
    if (buffer_ != nullptr) {
      buffer_->Destroy();
    }

    buffer_ = buffer;
  }

  Buffer* DecryptedBuffer() override { return buffer_; }

  void SetTimestamp(int64_t timestamp) { timestamp_ = timestamp; }

  int64_t Timestamp() const override { return timestamp_; }

 private:
  Buffer* buffer_;
  int64_t timestamp_;
};

std::unique_ptr<DecryptedBlockImpl> NewDecryptedBlockImpl();

// C++ implementation of the Host_10 interface which forwards calls to the
// actual implementation in the Rust HostImpl struct.
class Host10Impl : public Host_10 {
 public:
  Host10Impl(rust::Box<HostImpl>&& impl) : impl_(std::move(impl)) {}

  Buffer* Allocate(uint32_t capacity) override {
    return new BufferImpl(capacity);
  }

  void SetTimer(int64_t delay_ms, void* context) override {
    impl_->SetTimer(delay_ms, reinterpret_cast<uint64_t>(context));
  }

  Time GetCurrentWallTime() override { return impl_->GetCurrentWallTime(); }

  void OnInitialized(bool success) override { impl_->OnInitialized(success); }

  void OnResolveKeyStatusPromise(uint32_t promise_id,
                                 KeyStatus key_status) override {
    return impl_->OnResolveKeyStatusPromise(promise_id, key_status);
  }

  void OnResolveNewSessionPromise(uint32_t promise_id, const char* session_id,
                                  uint32_t session_id_size) override {
    impl_->OnResolveNewSessionPromise(promise_id, session_id, session_id_size);
  }

  void OnResolvePromise(uint32_t promise_id) override {
    impl_->OnResolvePromise(promise_id);
  }

  void OnRejectPromise(uint32_t promise_id, Exception exception,
                       uint32_t system_code, const char* error_message,
                       uint32_t error_message_size) override {
    impl_->OnRejectPromise(promise_id, exception, system_code, error_message,
                           error_message_size);
  }

  void OnSessionMessage(const char* session_id, uint32_t session_id_size,
                        MessageType message_type, const char* message,
                        uint32_t message_size) override {
    impl_->OnSessionMessage(session_id, session_id_size, message_type, message,
                            message_size);
  }

  void OnSessionKeysChange(const char* session_id, uint32_t session_id_size,
                           bool has_additional_usable_key,
                           const KeyInformation* keys_info,
                           uint32_t keys_info_count) override {
    impl_->OnSessionKeysChange(session_id, session_id_size,
                               has_additional_usable_key, keys_info,
                               keys_info_count);
  }

  void OnExpirationChange(const char* session_id, uint32_t session_id_size,
                          Time new_expiry_time) override {
    impl_->OnExpirationChange(session_id, session_id_size, new_expiry_time);
  }

  void OnSessionClosed(const char* session_id,
                       uint32_t session_id_size) override {
    impl_->OnSessionClosed(session_id, session_id_size);
  }

  void SendPlatformChallenge(const char* service_id, uint32_t service_id_size,
                             const char* challenge,
                             uint32_t challenge_size) override {
    std::cout << "TODO: SendPlatformChallenge\n";
  }

  void EnableOutputProtection(uint32_t desired_protection_mask) override {
    std::cout << "TODO: EnableOutputProtection\n";
  }

  void QueryOutputProtectionStatus() override {
    impl_->QueryOutputProtectionStatus();
  }

  void OnDeferredInitializationDone(StreamType stream_type,
                                    Status decoder_status) override {
    std::cout << "TODO: OnDeferredInitializationDone\n";
  }

  FileIO* CreateFileIO(FileIOClient* client) override {
    std::cout << "TODO: CreateFileIO\n";
    return nullptr;
  }

  void RequestStorageId(uint32_t version) override {
    std::cout << "TODO: RequestStorageId\n";
  }

 private:
  rust::Box<HostImpl> impl_;
};

class ContentDecryptionModule {
 public:
  ContentDecryptionModule(std::unique_ptr<Host10Impl> host,
                          ContentDecryptionModule_10* impl)
      : host_(std::move(host)), impl_(impl) {}

  ~ContentDecryptionModule() { impl_->Destroy(); }

  ContentDecryptionModule_10* Get() { return impl_; }

  void TimerExpired(uint64_t context) {
    Get()->TimerExpired(reinterpret_cast<void*>(context));
  }

 private:
  std::unique_ptr<Host10Impl> host_;

  // NOTE: Never null.
  ContentDecryptionModule_10* impl_;
};

// NOTE: Will return a nullptr on failure.
std::unique_ptr<ContentDecryptionModule> CreateCdm(rust::Box<HostImpl> host);

}  // namespace cdm