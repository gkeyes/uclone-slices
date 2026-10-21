#include "process_common.hpp"

#include <cstdio>
#include <sched.h>
#include <signal.h>
#include <sys/prctl.h>
#include <sys/wait.h>

#ifndef UCLONE_BUILD_ID
#error UCLONE_BUILD_ID is required
#endif

extern char** environ;

namespace {

constexpr const char* kImage = "/data/adb/modules/uclone-slices-v2/bin/uclone-namespace-anchor";

struct Record {
  std::string build;
  std::string boot;
  pid_t pid{};
  uint64_t started{};
  std::string ns;
};

std::string link_text(const std::string& path) {
  char buffer[512];
  auto count = readlink(path.c_str(), buffer, sizeof(buffer));
  if (count <= 0 || static_cast<size_t>(count) == sizeof(buffer)) return {};
  return {buffer, static_cast<size_t>(count)};
}

std::string boot_id() {
  std::string result = slice::read_file("/proc/sys/kernel/random/boot_id");
  if (!result.empty() && result.back() == '\n') result.pop_back();
  return result;
}

bool safe_root() {
  struct stat root{};
  return getuid() == 0 && lstat(std::string(slice::kRoot).c_str(), &root) == 0 &&
      S_ISDIR(root.st_mode) && root.st_uid == 0 && (root.st_mode & 0777) == 0700;
}

std::optional<Record> read_record() {
  std::string path = std::string(slice::kRoot) + "/namespace-anchor";
  slice::Fd fd(open(path.c_str(), O_RDONLY | O_CLOEXEC | O_NOFOLLOW));
  struct stat found{};
  if (!fd || fstat(fd.get(), &found) != 0 || !S_ISREG(found.st_mode) ||
      found.st_uid != 0 || (found.st_mode & 0777) != 0600 || found.st_size > 512) return {};
  char buffer[513];
  auto count = read(fd.get(), buffer, sizeof(buffer));
  if (count <= 0 || count > 512) return {};
  std::istringstream fields(std::string(buffer, static_cast<size_t>(count)));
  Record record;
  std::string extra;
  if (!(fields >> record.build >> record.boot >> record.pid >> record.started >> record.ns) || fields >> extra) return {};
  return record;
}

int open_anchor(int* root_fd = nullptr) {
  if (!safe_root()) return -1;
  auto record = read_record();
  if (!record || record->build != UCLONE_BUILD_ID || record->boot != boot_id() || record->pid <= 1 ||
      slice::start_time(record->pid) != record->started || slice::process_uid(record->pid) != 0) return -1;
  std::string proc = "/proc/" + std::to_string(record->pid);
  struct stat expected{}, actual{};
  if (stat(kImage, &expected) != 0 || stat((proc + "/exe").c_str(), &actual) != 0 ||
      !slice::same_object(expected, actual) || link_text(proc + "/ns/mnt") != record->ns ||
      record->ns == link_text("/proc/1/ns/mnt")) return -1;
  int fd = open((proc + "/ns/mnt").c_str(), O_RDONLY | O_CLOEXEC);
  if (fd < 0) return -1;
  int root = open((proc + "/root").c_str(), O_RDONLY | O_DIRECTORY | O_CLOEXEC);
  if (root < 0 || slice::start_time(record->pid) != record->started) {
    close(fd); if (root >= 0) close(root); return -1;
  }
  if (root_fd != nullptr) *root_fd = root; else close(root);
  return fd;
}

bool write_record() {
  auto started = slice::start_time(getpid());
  std::string ns = link_text("/proc/self/ns/mnt");
  if (!started || ns.empty()) return false;
  std::string text = std::string(UCLONE_BUILD_ID) + " " + boot_id() + " " +
      std::to_string(getpid()) + " " + std::to_string(*started) + " " + ns + "\n";
  std::string temporary = std::string(slice::kRoot) + "/namespace-anchor.pending";
  std::string target = std::string(slice::kRoot) + "/namespace-anchor";
  slice::Fd output(open(temporary.c_str(), O_WRONLY | O_CREAT | O_TRUNC | O_CLOEXEC | O_NOFOLLOW, 0600));
  if (!output || fchmod(output.get(), 0600) != 0) return false;
  size_t offset = 0;
  while (offset < text.size()) {
    auto count = write(output.get(), text.data() + offset, text.size() - offset);
    if (count < 0 && errno == EINTR) continue;
    if (count <= 0) return false;
    offset += static_cast<size_t>(count);
  }
  if (fsync(output.get()) != 0 || rename(temporary.c_str(), target.c_str()) != 0) return false;
  slice::Fd directory(open(std::string(slice::kRoot).c_str(), O_RDONLY | O_DIRECTORY | O_CLOEXEC));
  return directory && fsync(directory.get()) == 0;
}

bool start_anchor() {
  if (!safe_root()) return false;
  std::string self = link_text("/proc/self/ns/mnt");
  if (self.empty() || self == link_text("/proc/1/ns/mnt")) return false;
  slice::Fd existing(open_anchor());
  if (existing) {
    auto record = read_record();
    return record && record->ns == self;
  }
  auto previous = read_record();
  if (previous && previous->boot == boot_id() && previous->ns != self) {
    // The last namespace disappeared in this boot. Do not reconstruct a maintenance
    // or account view from an unrelated namespace while a helper may still use it.
    return false;
  }
  int ready[2];
  if (pipe2(ready, O_CLOEXEC) != 0) return false;
  pid_t child = fork();
  if (child < 0) { close(ready[0]); close(ready[1]); return false; }
  if (child == 0) {
    close(ready[0]);
    slice::Fd null(open("/dev/null", O_RDWR | O_CLOEXEC));
    if (!null) _exit(1);
    if (setsid() < 0 || !write_record()) _exit(1);
    (void)prctl(PR_SET_NAME, "uclone-ns", 0, 0, 0);
    char byte = 'R';
    if (write(ready[1], &byte, 1) != 1) _exit(1);
    close(ready[1]);
    for (int fd = 0; fd <= 2; ++fd) if (dup2(null.get(), fd) < 0) _exit(1);
    for (;;) pause();
  }
  close(ready[1]);
  pollfd item{ready[0], POLLIN, 0};
  int status = poll(&item, 1, slice::kFrameTimeoutMs);
  char byte{};
  bool ok = status > 0 && read(ready[0], &byte, 1) == 1 && byte == 'R';
  close(ready[0]);
  if (!ok) { (void)kill(child, SIGTERM); (void)waitpid(child, nullptr, 0); }
  return ok;
}

bool execute_helper(const char* image) {
  std::string_view path(image);
  constexpr std::string_view prefix = "/data/adb/uclone-slices-v2/manager-helper/";
  constexpr std::string_view suffix = "/uclone_archive";
  if (path.rfind(prefix, 0) != 0 || path.find("/../") != std::string_view::npos ||
      path.size() <= prefix.size() + suffix.size() ||
      path.substr(path.size() - suffix.size()) != suffix) return false;
  slice::Fd image_fd(open(image, O_RDONLY | O_NOFOLLOW | O_CLOEXEC));
  struct stat file{};
  if (!image_fd || fstat(image_fd.get(), &file) != 0 || !S_ISREG(file.st_mode) ||
      file.st_uid != 0 || (file.st_mode & 0777) != 0700) return false;
  int root = -1;
  slice::Fd ns(open_anchor(&root));
  slice::Fd target_root(root);
  if (!ns || !target_root || setns(ns.get(), CLONE_NEWNS) != 0 ||
      fchdir(target_root.get()) != 0 || chroot(".") != 0 || chdir("/") != 0) return false;
  char* args[]{const_cast<char*>(image), nullptr};
  (void)fexecve(image_fd.get(), args, environ);
  return false;
}

}  // namespace

int main(int argc, char** argv) {
  if (argc == 2 && std::string_view(argv[1]) == "--start") return start_anchor() ? 0 : 1;
  if (argc == 2 && std::string_view(argv[1]) == "--check") {
    slice::Fd ns(open_anchor());
    auto record = read_record();
    if (!ns || !record) return 1;
    std::printf("%d\n", record->pid);
    return 0;
  }
  if (argc == 3 && std::string_view(argv[1]) == "--exec-helper") return execute_helper(argv[2]) ? 0 : 1;
  return 2;
}
