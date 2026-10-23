#include "process_common.hpp"
#include "specialize_channel.hpp"
#include "zygisk.hpp"

#include <cstdio>
#include <cstdlib>
#include <sys/time.h>
#ifdef __ANDROID__
#include <android/log.h>
#endif

#ifndef UCLONE_BUILD_ID
#error UCLONE_BUILD_ID is required
#endif

namespace {

void rejected(const char* stage, int error, pid_t target = getpid()) {
  // Literal stages and errno only: no account names, permits or private paths.
#ifdef __ANDROID__
  __android_log_print(ANDROID_LOG_ERROR, "Slice", "native build=%s pid=%d stage=%s errno=%d",
                      UCLONE_BUILD_ID, target, stage, error);
#else
  std::fprintf(stderr, "native build=%s pid=%d stage=%s errno=%d\n",
               UCLONE_BUILD_ID, target, stage, error);
#endif
}

[[noreturn]] void abort_start(const char* stage, int error) {
  rejected(stage, error);
  _exit(126);
}

std::string java_string(JNIEnv* env, jstring value) {
  if (value == nullptr) return {};
  const char* chars = env->GetStringUTFChars(value, nullptr);
  if (chars == nullptr) { env->ExceptionClear(); return {}; }
  std::string result(chars);
  env->ReleaseStringUTFChars(value, chars);
  return result;
}

std::string package_for(JNIEnv* env, const zygisk::AppSpecializeArgs* args) {
  std::string directory = java_string(env, args->app_data_dir);
  for (std::string_view prefix : {"/data/user/0/", "/data/data/"}) {
    if (directory.rfind(prefix, 0) == 0) {
      std::string package = directory.substr(prefix.size());
      if (slice::package_name(package)) return package;
    }
  }
  std::string process = java_string(env, args->nice_name);
  process = process.substr(0, process.find(':'));
  constexpr std::string_view suffix = "_zygote";
  if (args->is_child_zygote != nullptr && *args->is_child_zygote &&
      process.size() > suffix.size() && process.compare(process.size() - suffix.size(), suffix.size(), suffix) == 0) {
    process.resize(process.size() - suffix.size());
  }
  return slice::package_name(process) ? process : std::string{};
}

bool enrolled(const std::string& package) {
  // This is discovery only. The Root runtime revalidates the complete aggregate,
  // installed UID, process identity and paired sources after specialization.
  std::string path = std::string(slice::kRoot) + "/packages/" + package + "/aggregate.json";
  struct stat found{};
  if (lstat(path.c_str(), &found) == 0) return true;  // malformed configured data fails closed later
  return errno != ENOENT && errno != ENOTDIR;
}

std::optional<bool> module_discovery(zygisk::Api* api, const std::string& package) {
  slice::Fd directory(api->getModuleDir());
  if (!directory) return {};
  slice::Fd input(openat(directory.get(), "enrollment.list", O_RDONLY | O_CLOEXEC | O_NOFOLLOW));
  if (!input) return {};
  std::string contents;
  char buffer[1024];
  for (;;) {
    auto count = read(input.get(), buffer, sizeof(buffer));
    if (count < 0 && errno == EINTR) continue;
    if (count < 0 || contents.size() + static_cast<size_t>(count) > 65536) return {};
    if (count == 0) break;
    contents.append(buffer, static_cast<size_t>(count));
  }
  std::istringstream lines(contents);
  std::string build, line;
  if (!std::getline(lines, build) || build != UCLONE_BUILD_ID) return {};
  bool found = false;
  while (std::getline(lines, line)) {
    std::istringstream fields(line);
    std::string candidate, uid, extra;
    if (!(fields >> candidate >> uid) || fields >> extra || !slice::package_name(candidate) ||
        !slice::number<uid_t>(uid)) return {};
    found = found || candidate == package;
  }
  return found;
}

bool mark_ready() {
  std::string root(slice::kRoot);
  if (mkdir(root.c_str(), 0700) != 0 && errno != EEXIST) return false;
  struct stat directory{};
  if (lstat(root.c_str(), &directory) != 0 || !S_ISDIR(directory.st_mode) ||
      directory.st_uid != 0 || (directory.st_mode & 0777) != 0700) return false;
  std::string boot = slice::read_file("/proc/sys/kernel/random/boot_id");
  if (!boot.empty() && boot.back() == '\n') boot.pop_back();
  if (boot.size() != 36) return false;
  slice::Fd fd(open((root + "/zygisk-ready").c_str(),
                    O_WRONLY | O_CREAT | O_TRUNC | O_NOFOLLOW | O_CLOEXEC, 0600));
  if (!fd) return false;
  std::string marker = std::string(UCLONE_BUILD_ID) + " " + boot + "\n";
  size_t offset = 0;
  while (offset < marker.size()) {
    auto count = write(fd.get(), marker.data() + offset, marker.size() - offset);
    if (count < 0 && errno == EINTR) continue;
    if (count <= 0) return false;
    offset += static_cast<size_t>(count);
  }
  return fsync(fd.get()) == 0;
}

class SliceModule : public zygisk::ModuleBase {
 public:
  void onLoad(zygisk::Api* api, JNIEnv* env) override { api_ = api; env_ = env; }

  void preServerSpecialize(zygisk::ServerSpecializeArgs*) override {
    slice::Fd fd(api_->connectCompanion());
    if (fd && slice::write_all(fd.get(), "SYSTEM " UCLONE_BUILD_ID "\n")) {
      (void)slice::frame(fd.get());
    }
    api_->setOption(zygisk::Option::DLCLOSE_MODULE_LIBRARY);
  }

  void preAppSpecialize(zygisk::AppSpecializeArgs* args) override {
    api_->setOption(zygisk::Option::DLCLOSE_MODULE_LIBRARY);
    // SDK sandboxes have their own OS-managed storage and must not receive App data.
    if (args->uid < 10000 || args->uid >= 100000 || (args->uid >= 20000 && args->uid < 30000)) return;
    package_ = package_for(env_, args);
    if (package_.empty() || package_ == "com.uclone.slices.v2") return;
    auto configured = module_discovery(api_, package_);
    if (configured == false) return;
    slice::Fd bootstrap(api_->connectCompanion());
    if (!bootstrap) {
      if (configured == true) abort_start("pre.connect", errno);
      // Unknown enrollment is guarded again before Application binding in system_server.
      return;
    }
    auto started = slice::start_time(getpid());
    if (!started) abort_start("pre.identity", errno);
    std::string hello = "APP " + std::to_string(getpid()) + " " +
        std::to_string(args->uid) + " " + std::to_string(*started) + " " + package_ + "\n";
    if (!slice::write_all(bootstrap.get(), hello)) abort_start("pre.hello", errno);
    auto response = slice::frame(bootstrap.get());
    if (response == "PASS") return;
    if (response != "MANAGED") abort_start("pre.managed", response ? EPROTO : errno);
    auto channel = slice::SpecializeChannel::create(bootstrap.get());
    if (!channel) abort_start("pre.channel", errno);
    response = slice::frame(bootstrap.get());
    if (response != "ARMED") abort_start("pre.armed", response ? EPROTO : errno);
    channel_ = std::move(*channel);
    // No descriptors escape this callback. Zygisk Next may close retained
    // companion sockets even when another implementation supports exemptFd.
  }

  void postAppSpecialize(const zygisk::AppSpecializeArgs*) override {
    if (!channel_) return;
    if (!channel_.request()) abort_start("post.request", errno);
    if (!channel_.await_approval()) abort_start("post.approval", errno);
    channel_.reset();
  }

 private:
  zygisk::Api* api_{};
  JNIEnv* env_{};
  slice::SpecializeChannel channel_;
  std::string package_;
};

void companion(int socket) {
  auto request = slice::frame(socket);
  if (!request) return;
  if (*request == "SYSTEM " UCLONE_BUILD_ID) {
    (void)slice::write_all(socket, mark_ready() ? "READY\n" : "DENY\n");
    return;
  }
  std::istringstream fields(*request);
  std::string operation, package, trailing;
  pid_t pid{};
  uid_t uid{};
  uint64_t started{};
  if (!(fields >> operation >> pid >> uid >> started >> package) || fields >> trailing ||
      operation != "APP" || pid <= 1 || uid < 10000 || uid >= 100000 ||
      !slice::package_name(package)) return;
  if (!enrolled(package)) { (void)slice::write_all(socket, "PASS\n"); return; }
  if (!slice::write_all(socket, "MANAGED\n")) { rejected("companion.managed", errno, pid); return; }
  auto channel = slice::SpecializeChannel::receive(socket);
  if (!channel) { rejected("companion.channel", errno, pid); return; }
  if (!slice::write_all(socket, "ARMED\n")) { rejected("companion.armed", errno, pid); return; }
  if (!channel->await_request()) {
    int error = errno;
    channel->refuse();
    rejected("companion.request", error, pid);
    return;
  }
  // Every subsequent failure wakes the App with a refusal rather than leaving
  // it waiting for the deadline. Only a confirmed Runtime result approves it.
  struct DenyUnlessApproved {
    slice::SpecializeChannel& channel;
    ~DenyUnlessApproved() { channel.deny(); }
  } refusal{*channel};
  if (slice::start_time(pid) != started || slice::process_uid(pid) != uid) {
    rejected("companion.identity", EACCES, pid); return;
  }
  slice::Fd control(slice::connect_control());
  if (!control) { rejected("companion.control_connect", errno, pid); return; }
  std::string frame = "{\"op\":\"specialize\",\"pid\":" + std::to_string(pid) +
      ",\"uid\":" + std::to_string(uid) + ",\"started\":" + std::to_string(started) +
      ",\"package\":\"" + package + "\"}\n";
  if (!slice::write_all(control.get(), frame)) { rejected("companion.control_write", errno, pid); return; }
  auto response = slice::frame(control.get(), slice::kRuntimeReplyTimeoutMs);
  if (response != "{\"ok\":{}}") {
    rejected("companion.control_reply", response ? EACCES : errno, pid); return;
  }
  if (!channel->approve()) rejected("companion.approval", errno, pid);
}

}  // namespace

REGISTER_ZYGISK_MODULE(SliceModule)
REGISTER_ZYGISK_COMPANION(companion)
