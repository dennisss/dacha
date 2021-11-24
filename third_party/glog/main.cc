#include <glog/logging.h>

int main(int argc, char* argv[]) {
    // Initialize Google’s logging library.
    google::InitGoogleLogging(argv[0]);

    LOG(INFO) << "Testing logging of" << 15 << " cookies";
    LOG(ERROR) << "And this is an error!";
}
