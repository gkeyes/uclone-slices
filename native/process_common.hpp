#pragma once

#include <cerrno>
#include <charconv>
#include <cstdint>
#include <cstddef>
#include <ctime>
#include <cstring>
#include <fcntl.h>
#include <fstream>
#include <optional>
#include <poll.h>
#include <sstream>
#include <string>
#include <string_view>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/un.h>
#include <unistd.h>
#include <vector>
#include <utility>

namespace slice {

constexpr std::string_view kSocket = "uclone.slices.launch.v1";
constexpr std::string_view kRoot = "/data/adb/uclone-slices-v2";
// The control transport uses the same whole-frame bound as the Runtime RPC.
constexpr int kFrameTimeoutMs = 5000;

class Fd {
 public:
  explicit Fd(int value = -1) : value_(value) {}
  ~Fd() { if (value_ >= 0) close(value_); }
  Fd(const Fd&) = delete;
  Fd& operator=(const Fd&) = delete;
  Fd(Fd&& other) noexcept : value_(std::exchange(other.value_, -1)) {}
  int get() const { return value_; }
  explicit operator bool() const { return value_ >= 0; }
 private:
  int value_;
};

inline bool package_name(std::string_view text) {
  if (text.empty() || text.size() > 255 || text.front() == '.' || text.back() == '.') return false;
  bool dot = false;
  bool component = false;
  for (char ch : text) {
    if (ch == '.') {
      if (!component) return false;
      dot = true;
      component = false;
    } else if ((ch >= 'a' && ch <= 'z') || (ch >= 'A' && ch <= 'Z') ||
               (ch >= '0' && ch <= '9') || ch == '_') {
      component = true;
    } else return false;
  }
  return dot && component;
}

inline bool slot_name(std::string_view text) {
  if (text == "base") return true;
  if (text.empty() || text.size() > 128) return false;
  for (char ch : text) {
    if (!((ch >= 'a' && ch <= 'z') || (ch >= 'A' && ch <= 'Z') ||
          (ch >= '0' && ch <= '9') || ch == '-' || ch == '_')) return false;
  }
  return true;
}

template <typename T>
inline std::optional<T> number(std::string_view text) {
  T value{};
  auto result = std::from_chars(text.data(), text.data() + text.size(), value);
  if (text.empty() || result.ec != std::errc{} || result.ptr != text.data() + text.size()) return {};
  return value;
}

inline std::string read_file(const std::string& path) {
  std::ifstream input(path);
  return {std::istreambuf_iterator<char>(input), std::istreambuf_iterator<char>()};
}

inline std::optional<uint64_t> start_time(pid_t pid) {
  std::string stat = read_file("/proc/" + std::to_string(pid) + "/stat");
  auto end = stat.rfind(')');
  if (end == std::string::npos) return {};
  std::istringstream fields(stat.substr(end + 1));
  std::string value;
  // The first token after comm is field 3; starttime is field 22.
  for (int field = 3; field <= 22; ++field) if (!(fields >> value)) return {};
  return number<uint64_t>(value);
}

inline std::optional<uid_t> process_uid(pid_t pid) {
  std::istringstream lines(read_file("/proc/" + std::to_string(pid) + "/status"));
  std::string line;
  while (std::getline(lines, line)) {
    if (line.rfind("Uid:", 0) != 0) continue;
    std::istringstream fields(line.substr(4));
    uid_t real{}, effective{}, saved{}, filesystem{};
    if (fields >> real >> effective >> saved >> filesystem && real == effective &&
        real == saved && real == filesystem) return real;
    return {};
  }
  return {};
}

inline bool write_all(int fd, std::string_view text) {
  while (!text.empty()) {
    auto written = send(fd, text.data(), text.size(), MSG_NOSIGNAL);
    if (written < 0 && errno == EINTR) continue;
    if (written <= 0) return false;
    text.remove_prefix(static_cast<size_t>(written));
  }
  return true;
}

inline std::optional<std::string> frame(int fd) {
  std::string result;
  // A deadline for the whole frame, rather than one timeout for every byte.
  timespec begin{};
  if (clock_gettime(CLOCK_MONOTONIC, &begin) != 0) return {};
  while (result.size() < 65536) {
    timespec now{};
    if (clock_gettime(CLOCK_MONOTONIC, &now) != 0) return {};
    int elapsed = static_cast<int>((now.tv_sec - begin.tv_sec) * 1000 +
                                  (now.tv_nsec - begin.tv_nsec) / 1000000);
    int remaining = kFrameTimeoutMs - elapsed;
    if (remaining <= 0) { errno = ETIMEDOUT; return {}; }
    pollfd item{fd, POLLIN, 0};
    int ready = poll(&item, 1, remaining);
    if (ready < 0 && errno == EINTR) continue;
    if (ready == 0) { errno = ETIMEDOUT; return {}; }
    if (ready < 0) return {};
    char ch{};
    auto count = read(fd, &ch, 1);
    if (count < 0 && errno == EINTR) continue;
    if (count == 0) { errno = ECONNRESET; return {}; }
    if (count != 1) return {};
    if (ch == '\n') return result;
    result.push_back(ch);
  }
  errno = EMSGSIZE;
  return {};
}

inline int connect_control() {
  Fd candidate(socket(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0));
  if (!candidate) return -1;
  sockaddr_un address{};
  address.sun_family = AF_UNIX;
  std::memcpy(address.sun_path + 1, kSocket.data(), kSocket.size());
  auto length = static_cast<socklen_t>(offsetof(sockaddr_un, sun_path) + 1 + kSocket.size());
  timeval timeout{5, 0};
  if (setsockopt(candidate.get(), SOL_SOCKET, SO_SNDTIMEO, &timeout, sizeof(timeout)) != 0 ||
      connect(candidate.get(), reinterpret_cast<sockaddr*>(&address), length) != 0) return -1;
  // Any process can bind an abstract name first; only the Root daemon may answer.
  ucred peer{};
  socklen_t size = sizeof(peer);
  if (getsockopt(candidate.get(), SOL_SOCKET, SO_PEERCRED, &peer, &size) != 0) return -1;
  if (size != sizeof(peer) || peer.uid != 0) { errno = EACCES; return -1; }
  return dup(candidate.get());
}

inline bool same_object(const struct stat& first, const struct stat& second) {
  return first.st_dev == second.st_dev && first.st_ino == second.st_ino;
}

}  // namespace slice
