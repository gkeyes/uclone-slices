// Runs the production module AND companion callbacks in a disposable Linux PID,
// mount and network namespace. Only Android's argument/JNI/loader plumbing and the
// final Runtime authorization reply are supplied by the fixture.
#include "../native/slice_zygisk.cpp"

#include <cstdio>
#include <dirent.h>
#include <filesystem>
#include <sys/mount.h>
#include <sys/wait.h>

namespace {
constexpr uid_t kAppUid = 10001;
constexpr char kPackage[] = "com.example.slice_fixture";
zygisk::internal::module_abi* registered{};
int returned_companion = -1;
struct stat returned_identity{};
bool reject_exemption = false;

bool register_module(zygisk::internal::api_table*, zygisk::internal::module_abi* module) {
  registered = module;
  return true;
}

int connect_companion(void*) {
  int pair[2];
  if (socketpair(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0, pair) != 0) return -1;
  pid_t child = fork();
  if (child == 0) {
    close(pair[0]);
    companion(pair[1]);
    close(pair[1]);
    _exit(0);
  }
  close(pair[1]);
  if (child < 0) { close(pair[0]); return -1; }
  returned_companion = pair[0];
  if (fstat(pair[0], &returned_identity) != 0) _exit(90);
  return pair[0];
}

// Zygisk Next documents closing leftover companion descriptors at the pre/post
// boundary. A loader must not close a descriptor number that has since been reused.
void close_retained_companion() {
  struct stat current{};
  if (returned_companion >= 0 && fstat(returned_companion, &current) == 0 &&
      slice::same_object(current, returned_identity)) close(returned_companion);
}

size_t descriptor_count() {
  DIR* directory = opendir("/proc/self/fd");
  if (!directory) _exit(91);
  size_t count = 0;
  while (auto* item = readdir(directory)) if (item->d_name[0] != '.') ++count;
  closedir(directory);
  return count;
}

// The public API deliberately deletes its constructor. This is the same reference
// layout a Zygisk loader supplies, not a second module implementation.
struct LoaderArgs {
  jint &uid, &gid;
  jintArray &gids;
  jint &runtime_flags;
  jobjectArray &rlimits;
  jint &mount_external;
  jstring &se_info, &nice_name, &instruction_set, &app_data_dir;
  jintArray* const fds_to_ignore;
  jboolean* const is_child_zygote;
  jboolean* const is_top_app;
  jobjectArray* const pkg_data_info_list;
  jobjectArray* const whitelisted_data_info_list;
  jboolean* const mount_data_dirs;
  jboolean* const mount_storage_dirs;
};
static_assert(sizeof(LoaderArgs) == sizeof(zygisk::AppSpecializeArgs));

[[noreturn]] void application(bool wrong_uid) {
  JNINativeInterface_ jni{};
  jni.GetStringUTFChars = [](JNIEnv*, jstring text, jboolean*) {
    return reinterpret_cast<const char*>(text);
  };
  jni.ReleaseStringUTFChars = [](JNIEnv*, jstring, const char*) {};
  jni.ExceptionClear = [](JNIEnv*) {};
  JNIEnv env{&jni};
  zygisk::internal::api_table api{};
  api.registerModule = register_module;
  api.connectCompanion = connect_companion;
  api.getModuleDir = [](void*) { return open("/module", O_RDONLY | O_DIRECTORY | O_CLOEXEC); };
  api.setOption = [](void*, zygisk::Option) {};
  api.exemptFd = [](int) { return !reject_exemption; };
  zygisk_module_entry(&api, &env);
  if (!registered) _exit(92);
  jint uid = kAppUid, gid = kAppUid, flags = 0, external = 0;
  jintArray groups = nullptr, ignored = nullptr;
  jobjectArray limits = nullptr;
  jstring se_info = nullptr, isa = nullptr;
  jstring name = reinterpret_cast<jstring>(const_cast<char*>(kPackage));
  jstring directory = reinterpret_cast<jstring>(const_cast<char*>("/data/user/0/com.example.slice_fixture"));
  LoaderArgs storage{uid, gid, groups, flags, limits, external, se_info, name, isa, directory,
                     &ignored, nullptr, nullptr, nullptr, nullptr, nullptr, nullptr};
  auto* args = reinterpret_cast<zygisk::AppSpecializeArgs*>(&storage);
  size_t initial_fds = descriptor_count();
  registered->preAppSpecialize(registered->impl, args);
  size_t after_pre_fds = descriptor_count();
  close_retained_companion();
  uid_t specialized = wrong_uid ? kAppUid + 1 : kAppUid;
  if (setresgid(specialized, specialized, specialized) != 0 ||
      setresuid(specialized, specialized, specialized) != 0) _exit(93);
  registered->postAppSpecialize(registered->impl, args);
  if (after_pre_fds != initial_fds || descriptor_count() != initial_fds) _exit(94);
  // Reaching here represents permission to enter application code.
  _exit(0);
}

int runtime_listener(bool squatted) {
  // SO_PEERCRED reports the effective UID at listen(): a squatting App owns the name.
  if (squatted && seteuid(kAppUid + 7) != 0) std::exit(95);
  int fd = socket(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0);
  sockaddr_un address{};
  address.sun_family = AF_UNIX;
  std::memcpy(address.sun_path + 1, slice::kSocket.data(), slice::kSocket.size());
  socklen_t length = offsetof(sockaddr_un, sun_path) + 1 + slice::kSocket.size();
  if (fd < 0 || bind(fd, reinterpret_cast<sockaddr*>(&address), length) != 0 || listen(fd, 1) != 0) {
    std::perror("fixture listener");
    std::exit(95);
  }
  if (squatted && seteuid(0) != 0) std::exit(95);
  return fd;
}

[[noreturn]] void runtime_peer(int listener, std::string_view scenario) {
  slice::Fd client(accept4(listener, nullptr, nullptr, SOCK_CLOEXEC));
  close(listener);
  if (!client) _exit(96);
  auto request = slice::frame(client.get());
  if (!request || request->find("\"op\":\"specialize\"") == std::string::npos ||
      request->find(kPackage) == std::string::npos) _exit(97);
  slice::Fd seen(open("/rpc-seen", O_WRONLY | O_CREAT | O_EXCL, 0600));
  if (!seen) _exit(98);
  if (scenario == "timeout") for (;;) pause();
  if (scenario != "disconnect") {
    auto reply = scenario == "reject" ? "{\"error\":{\"code\":\"launch_unavailable\"}}\n" : "{\"ok\":{}}\n";
    if (!slice::write_all(client.get(), reply)) _exit(99);
  }
  _exit(0);
}
}  // namespace

int main(int argc, char** argv) {
  // Refuse to set up fixed /data paths or mounts outside the caller's disposable lab.
  if (argc != 3 || getuid() != 0 || getpid() != 1) return 2;
  std::string_view scenario(argv[2]);
  if (scenario != "accept" && scenario != "no_exemption" && scenario != "reject" &&
      scenario != "wrong_uid" && scenario != "disconnect" && scenario != "timeout" &&
      scenario != "squatted") return 2;
  if (mount(nullptr, "/", nullptr, MS_PRIVATE | MS_REC, nullptr) != 0) return 3;
  std::filesystem::create_directories(std::string(argv[1]) + "/proc");
  if (chroot(argv[1]) != 0 || chdir("/") != 0 || mount("proc", "/proc", "proc", 0, nullptr) != 0) return 4;
  std::filesystem::create_directories("/module");
  std::filesystem::create_directories(std::string(slice::kRoot) + "/packages/" + kPackage);
  std::ofstream("/module/enrollment.list") << UCLONE_BUILD_ID "\n" << kPackage << " " << kAppUid << '\n';
  std::ofstream(std::string(slice::kRoot) + "/packages/" + kPackage + "/aggregate.json") << "{}";
  int listener = runtime_listener(scenario == "squatted");
  pid_t server = fork();
  if (server == 0) runtime_peer(listener, scenario);
  if (server < 0) return 5;
  close(listener);
  reject_exemption = scenario == "no_exemption";
  pid_t app = fork();
  if (app == 0) application(scenario == "wrong_uid");
  if (app < 0) return 6;
  int status{};
  if (waitpid(app, &status, 0) != app) return 7;
  bool approved = scenario == "accept" || scenario == "no_exemption";
  bool called = access("/rpc-seen", F_OK) == 0;
  int expected = approved ? 0 : 126;
  int actual = WIFEXITED(status) ? WEXITSTATUS(status) : -WTERMSIG(status);
  bool passed = actual == expected && called == (scenario != "wrong_uid" && scenario != "squatted");
  std::printf("native lifecycle %s: exit=%d expected=%d runtime_called=%d %s\n", argv[2],
              actual, expected, called, passed ? "PASS" : "FAIL");
  return passed ? 0 : 1;
}
