#include "process_common.hpp"

#include <cstdio>
#include <sched.h>
#include <sys/mount.h>
#include <sys/syscall.h>
#include <linux/mount.h>

namespace {

struct Request {
  pid_t pid;
  uid_t uid;
  uint64_t started;
  std::string package;
  std::string slot;
  bool inherited;
  bool isolated;
};

bool identity_matches(const Request& request) {
  return slice::start_time(request.pid) == request.started &&
         slice::process_uid(request.pid) == request.uid;
}

bool sources_match(const std::vector<std::string>& paths, const struct stat& source) {
  for (const auto& path : paths) {
    struct stat found{};
    if (stat(path.c_str(), &found) != 0 || !slice::same_object(found, source)) return false;
  }
  return true;
}

std::optional<std::vector<std::string>> aliases(const std::string& package, bool ce, bool isolated) {
  std::vector<std::string> candidates{
      (ce ? "/data/user/0/" : "/data/user_de/0/") + package};
  std::vector<std::string> optional = ce
      ? std::vector<std::string>{"/data/data/" + package,
                                "/data_mirror/data_ce/null/0/" + package,
                                "/data_mirror/data_ce/0/" + package}
      : std::vector<std::string>{"/data_mirror/data_de/null/0/" + package,
                                "/data_mirror/data_de/0/" + package};
  candidates.insert(candidates.end(), optional.begin(), optional.end());
  std::vector<std::string> result;
  for (size_t index = 0; index < candidates.size(); ++index) {
    const auto& path = candidates[index];
    struct stat found{};
    int result_code = stat(path.c_str(), &found);
    if (result_code == 0) {
      if (!S_ISDIR(found.st_mode)) return std::nullopt;
      result.push_back(path);
    } else if ((errno != ENOENT && errno != ENOTDIR) || (index == 0 && !isolated)) return std::nullopt;
  }
  return result;
}

std::optional<std::vector<slice::Fd>> clone_sources(int source, size_t count) {
  std::vector<slice::Fd> result;
  result.reserve(count);
  for (size_t index = 0; index < count; ++index) {
    int fd = static_cast<int>(syscall(__NR_open_tree, source, "",
        AT_EMPTY_PATH | OPEN_TREE_CLONE | OPEN_TREE_CLOEXEC));
    if (fd < 0) {
      if (errno == ENOSYS || errno == EOPNOTSUPP) return std::vector<slice::Fd>{};
      return std::nullopt;
    }
    result.emplace_back(fd);
  }
  return result;
}

bool bind_paths(const std::string& source, const struct stat& expected,
                const std::vector<slice::Fd>& trees, const std::vector<std::string>& paths) {
  if (sources_match(paths, expected)) return true;
  for (size_t index = 0; index < paths.size(); ++index) {
    if (!trees.empty()) {
      if (index >= trees.size() || syscall(__NR_move_mount, trees[index].get(), "",
          AT_FDCWD, paths[index].c_str(), MOVE_MOUNT_F_EMPTY_PATH) != 0) return false;
    } else {
      // Linux before 5.2 has no detached mount API. It can bind only a source
      // visible in this very namespace; a foreign /proc/PID/fd path is rejected.
      struct stat actual{};
      if (stat(source.c_str(), &actual) != 0 || !slice::same_object(actual, expected) ||
          mount(source.c_str(), paths[index].c_str(), nullptr, MS_BIND, nullptr) != 0) return false;
    }
  }
  return true;
}

bool install(const Request& request, const char*& stage) {
  stage = "identity";
  if (getuid() != 0 || request.pid <= 1 || !identity_matches(request)) return false;
  std::string proc = "/proc/" + std::to_string(request.pid);
  slice::Fd target(open((proc + "/ns/mnt").c_str(), O_RDONLY | O_CLOEXEC));
  slice::Fd target_root(open((proc + "/root").c_str(), O_RDONLY | O_DIRECTORY | O_CLOEXEC));
  slice::Fd init(open("/proc/1/ns/mnt", O_RDONLY | O_CLOEXEC));
  slice::Fd init_root(open("/proc/1/root", O_RDONLY | O_DIRECTORY | O_CLOEXEC));
  slice::Fd current(open("/proc/self/ns/mnt", O_RDONLY | O_CLOEXEC));
  struct stat target_stat{}, init_stat{}, current_stat{};
  if (!target || !target_root || !init || !init_root || !current || fstat(target.get(), &target_stat) != 0 ||
      fstat(init.get(), &init_stat) != 0 || fstat(current.get(), &current_stat) != 0 ||
      slice::same_object(target_stat, init_stat) ||
      slice::same_object(target_stat, current_stat)) return false;

  // Read sources in init's untouched view and clone detached mount trees there.
  // Passing an ordinary directory FD across setns is insufficient: MS_BIND rejects
  // source mounts belonging to another namespace. No bind is installed in init.
  stage = "source-namespace";
  if (setns(init.get(), CLONE_NEWNS) != 0 || fchdir(init_root.get()) != 0 ||
      chroot(".") != 0 || chdir("/") != 0) return false;
  std::string ce_path = request.slot == "base"
      ? "/data/user/0/" + request.package
      : "/data/misc_ce/0/uclone-slices-v2/slots/" + request.package + "/" + request.slot;
  std::string de_path = request.slot == "base"
      ? "/data/user_de/0/" + request.package
      : "/data/misc_de/0/uclone-slices-v2/slots/" + request.package + "/" + request.slot;
  slice::Fd ce(open(ce_path.c_str(), O_RDONLY | O_DIRECTORY | O_CLOEXEC | O_NOFOLLOW));
  slice::Fd de(open(de_path.c_str(), O_RDONLY | O_DIRECTORY | O_CLOEXEC | O_NOFOLLOW));
  struct stat ce_stat{}, de_stat{};
  stage = "sources";
  if (!ce || !de || fstat(ce.get(), &ce_stat) != 0 || fstat(de.get(), &de_stat) != 0) return false;
  stage = "source-trees";
  // One detached tree per canonical/legacy/mirror alias. An inherited view is only
  // verified and never changes its parent's namespace.
  auto ce_trees = clone_sources(ce.get(), request.inherited ? 0 : 4);
  auto de_trees = clone_sources(de.get(), request.inherited ? 0 : 3);
  if (!ce_trees || !de_trees) return false;
  stage = "namespace";
  if (!identity_matches(request) || setns(target.get(), CLONE_NEWNS) != 0) return false;
  stage = "root";
  if (fchdir(target_root.get()) != 0 || chroot(".") != 0 || chdir("/") != 0) return false;
  stage = "aliases";
  auto ce_aliases = aliases(request.package, true, request.isolated);
  auto de_aliases = aliases(request.package, false, request.isolated);
  if (!ce_aliases || !de_aliases) return false;
  if (request.inherited) {
    // A fork sharing a verified App Zygote namespace may only verify it. It must not
    // replace a mount still used by its parent or siblings.
    return sources_match(*ce_aliases, ce_stat) && sources_match(*de_aliases, de_stat) &&
           identity_matches(request);
  }
  // Retain incoming Android volume updates while preventing account mounts from
  // propagating out to init/Zygote or another App namespace.
  stage = "propagation";
  if (mount(nullptr, "/", nullptr, MS_SLAVE | MS_REC, nullptr) != 0) return false;
  // Android may mask every private-data path of an isolated UID. Keep that masking;
  // never create or expose an App data directory merely to make a mount succeed.
  stage = "ce-bind";
  if (!bind_paths(ce_path, ce_stat, *ce_trees, *ce_aliases)) return false;
  stage = "de-bind";
  if (!bind_paths(de_path, de_stat, *de_trees, *de_aliases)) return false;
  stage = "verification";
  return sources_match(*ce_aliases, ce_stat) && sources_match(*de_aliases, de_stat) &&
         identity_matches(request);
}

}  // namespace

int main(int argc, char** argv) {
  if (argc < 7 || argc > 9) return 2;
  auto pid = slice::number<pid_t>(argv[1]);
  auto uid = slice::number<uid_t>(argv[2]);
  auto started = slice::number<uint64_t>(argv[3]);
  if (!pid || !uid || !started || !slice::package_name(argv[4]) ||
      !slice::slot_name(argv[5]) || std::string_view(argv[6]) != "--install") return 2;
  bool inherited = false, isolated = false;
  for (int index = 7; index < argc; ++index) {
    if (std::string_view(argv[index]) == "--verify-inherited" && !inherited) inherited = true;
    else if (std::string_view(argv[index]) == "--isolated" && !isolated) isolated = true;
    else return 2;
  }
  Request request{*pid, *uid, *started, argv[4], argv[5], inherited, isolated};
  const char* stage = "request";
  if (!install(request, stage)) {
    // No source paths, account names, or process payloads are written to logs.
    std::fprintf(stderr, "process view refused at %s (%d)\n", stage, errno);
    return 1;
  }
  return 0;
}
