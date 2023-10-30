#include "ffi.h"

#include <iostream>

namespace cdm {
namespace {

void* GetCdmHostFuncImpl(int host_interface_version, void* user_data) {
  if (host_interface_version != Host_10::kVersion) {
    std::cout << "Unsupported host version\n";
    return nullptr;
  }

  return user_data;
}

}  // namespace

std::unique_ptr<ContentDecryptionModule> CreateCdm(rust::Box<HostImpl> host) {
  auto host10 = std::make_unique<Host10Impl>(std::move(host));

  const std::string key_system = "com.widevine.alpha";

  void* inst = CreateCdmInstance(ContentDecryptionModule_10::kVersion,
                                 key_system.c_str(), key_system.length(),
                                 GetCdmHostFuncImpl, host10.get());
  if (inst == nullptr) {
    return nullptr;
  }

  auto out = std::make_unique<ContentDecryptionModule>(
      std::move(host10), static_cast<ContentDecryptionModule_10*>(inst));
  return out;
}

std::unique_ptr<DecryptedBlockImpl> NewDecryptedBlockImpl() {
  return std::make_unique<DecryptedBlockImpl>();
}

}  // namespace cdm
