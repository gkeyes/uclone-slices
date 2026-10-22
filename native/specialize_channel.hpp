#pragma once

#include "process_common.hpp"

#include <climits>
#include <linux/futex.h>
#include <linux/memfd.h>
#include <sys/mman.h>
#include <sys/syscall.h>

namespace slice {

// A mapping survives specialization without retaining any descriptor. The Root
// companion maps the same sealed-size memfd during preAppSpecialize; both sides
// close their memfd, and the App closes its bootstrap socket before returning.
// No account path, permission or file data is stored here.
class SpecializeChannel {
 public:
  SpecializeChannel() = default;
  ~SpecializeChannel() { reset(); }
  SpecializeChannel(const SpecializeChannel&) = delete;
  SpecializeChannel& operator=(const SpecializeChannel&) = delete;
  SpecializeChannel(SpecializeChannel&& other) noexcept
      : state_(std::exchange(other.state_, nullptr)) {}
  SpecializeChannel& operator=(SpecializeChannel&& other) noexcept {
    if (this != &other) { reset(); state_ = std::exchange(other.state_, nullptr); }
    return *this;
  }
  explicit operator bool() const { return state_ != nullptr; }
  void reset() {
    if (state_ != nullptr) munmap(std::exchange(state_, nullptr), sizeof(uint32_t));
  }

  static std::optional<SpecializeChannel> create(int socket) {
    Fd fd(static_cast<int>(syscall(__NR_memfd_create, "uclone.launch", MFD_CLOEXEC | MFD_ALLOW_SEALING)));
    if (!fd || ftruncate(fd.get(), sizeof(uint32_t)) != 0 ||
        fcntl(fd.get(), F_ADD_SEALS, kSeals) != 0) return {};
    auto channel = map(fd.get());
    if (!channel || !send_descriptor(socket, fd.get())) return {};
    return channel;
  }

  static std::optional<SpecializeChannel> receive(int socket) {
    Fd fd(receive_descriptor(socket));
    if (!fd) return {};
    struct stat info{};
    if (fstat(fd.get(), &info) != 0) return {};
    int seals = fcntl(fd.get(), F_GET_SEALS);
    // New kernels can additionally apply F_SEAL_EXEC through memfd_noexec.
    if (!S_ISREG(info.st_mode) || info.st_size != static_cast<off_t>(sizeof(uint32_t)) ||
        seals < 0 || (seals & kSeals) != kSeals) { errno = EPROTO; return {}; }
    auto channel = map(fd.get());
    if (channel && channel->phase() != kPending) { errno = EPROTO; return {}; }
    return channel;
  }

  bool request() { return transition(kPending, kRequested); }
  bool await_request() { return wait_changed(kPending) == kRequested; }
  bool approve() { return transition(kRequested, kApproved); }
  void deny() { (void)transition(kRequested, kDenied); }
  bool await_approval() {
    uint32_t result = wait_changed(kRequested);
    if (result == kApproved) return true;
    if (result == kDenied) errno = EACCES;
    else if (result != kInvalid) errno = EPROTO;
    return false;
  }

 private:
  static constexpr int kSeals = F_SEAL_SHRINK | F_SEAL_GROW | F_SEAL_SEAL;
  static constexpr uint32_t kPending = 0, kRequested = 1, kApproved = 2, kDenied = 3;
  static constexpr uint32_t kInvalid = UINT32_MAX;
  static_assert(__atomic_always_lock_free(sizeof(uint32_t), nullptr));
  uint32_t* state_{};

  static std::optional<SpecializeChannel> map(int fd) {
    void* address = mmap(nullptr, sizeof(uint32_t), PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    if (address == MAP_FAILED) return {};
    SpecializeChannel result;
    result.state_ = static_cast<uint32_t*>(address);
    return result;
  }

  uint32_t phase() const { return __atomic_load_n(state_, __ATOMIC_ACQUIRE); }
  bool transition(uint32_t expected, uint32_t next) {
    if (!state_ || !__atomic_compare_exchange_n(state_, &expected, next, false,
                                               __ATOMIC_RELEASE, __ATOMIC_RELAXED)) {
      errno = EPROTO;
      return false;
    }
    // Shared FUTEX operations: PRIVATE_FLAG would not wake a different process.
    (void)syscall(__NR_futex, state_, FUTEX_WAKE, INT_MAX, nullptr, nullptr, 0);
    return true;
  }

  uint32_t wait_changed(uint32_t expected) {
    timespec deadline{};
    if (!state_ || clock_gettime(CLOCK_MONOTONIC, &deadline) != 0) return kInvalid;
    deadline.tv_sec += kFrameTimeoutMs / 1000;
    deadline.tv_nsec += (kFrameTimeoutMs % 1000) * 1000000L;
    if (deadline.tv_nsec >= 1000000000L) { ++deadline.tv_sec; deadline.tv_nsec -= 1000000000L; }
    for (;;) {
      uint32_t value = phase();
      if (value != expected) return value;
      timespec now{}, remaining{};
      if (clock_gettime(CLOCK_MONOTONIC, &now) != 0) return kInvalid;
      remaining.tv_sec = deadline.tv_sec - now.tv_sec;
      remaining.tv_nsec = deadline.tv_nsec - now.tv_nsec;
      if (remaining.tv_nsec < 0) { --remaining.tv_sec; remaining.tv_nsec += 1000000000L; }
      if (remaining.tv_sec < 0 || (remaining.tv_sec == 0 && remaining.tv_nsec == 0)) {
        errno = ETIMEDOUT;
        return kInvalid;
      }
      if (syscall(__NR_futex, state_, FUTEX_WAIT, expected, &remaining, nullptr, 0) == 0) continue;
      if (errno != EINTR && errno != EAGAIN) return kInvalid;
    }
  }

  static bool send_descriptor(int socket, int fd) {
    char marker = 'S';
    iovec data{&marker, 1};
    alignas(cmsghdr) char control[CMSG_SPACE(sizeof(int))]{};
    msghdr message{};
    message.msg_iov = &data;
    message.msg_iovlen = 1;
    message.msg_control = control;
    message.msg_controllen = sizeof(control);
    cmsghdr* header = CMSG_FIRSTHDR(&message);
    header->cmsg_level = SOL_SOCKET;
    header->cmsg_type = SCM_RIGHTS;
    header->cmsg_len = CMSG_LEN(sizeof(int));
    std::memcpy(CMSG_DATA(header), &fd, sizeof(fd));
    ssize_t count;
    do { count = sendmsg(socket, &message, MSG_NOSIGNAL); } while (count < 0 && errno == EINTR);
    return count == 1;
  }

  static int receive_descriptor(int socket) {
    // Only the trusted pre-specialization peer can reach this bootstrap. Bound
    // its single descriptor message with the same transport deadline as frames.
    timeval timeout{kFrameTimeoutMs / 1000, (kFrameTimeoutMs % 1000) * 1000};
    if (setsockopt(socket, SOL_SOCKET, SO_RCVTIMEO, &timeout, sizeof(timeout)) != 0) return -1;
    char marker{};
    iovec data{&marker, 1};
    alignas(cmsghdr) char control[CMSG_SPACE(sizeof(int) * 4)]{};
    msghdr message{};
    message.msg_iov = &data;
    message.msg_iovlen = 1;
    message.msg_control = control;
    message.msg_controllen = sizeof(control);
    ssize_t count;
    do { count = recvmsg(socket, &message, MSG_CMSG_CLOEXEC); } while (count < 0 && errno == EINTR);
    if (count < 0) return -1;
    std::vector<int> descriptors;
    for (cmsghdr* header = CMSG_FIRSTHDR(&message); header != nullptr;
         header = CMSG_NXTHDR(&message, header)) {
      if (header->cmsg_level != SOL_SOCKET || header->cmsg_type != SCM_RIGHTS ||
          header->cmsg_len < CMSG_LEN(0)) continue;
      size_t bytes = header->cmsg_len - CMSG_LEN(0);
      for (size_t offset = 0; offset + sizeof(int) <= bytes; offset += sizeof(int)) {
        int fd;
        std::memcpy(&fd, CMSG_DATA(header) + offset, sizeof(fd));
        descriptors.push_back(fd);
      }
    }
    if (count != 1 || marker != 'S' || (message.msg_flags & (MSG_CTRUNC | MSG_TRUNC)) ||
        descriptors.size() != 1) {
      for (int fd : descriptors) close(fd);
      errno = EPROTO;
      return -1;
    }
    return descriptors.front();
  }
};
}  // namespace slice
