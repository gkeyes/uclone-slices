#include <jni.h>

#include <cerrno>
#include <cstring>
#include <fcntl.h>
#include <string>
#include <sys/mman.h>
#include <sys/stat.h>
#include <unistd.h>

namespace {
constexpr char kMarkerFile[] = "native-slot-marker.txt";
constexpr char kMmapFile[] = "native-mmap-marker.bin";
constexpr size_t kMmapSize = 4096;
constexpr size_t kMaximumMarkerLength = 64;

class ScopedFd {
public:
    explicit ScopedFd(int fd) : fd_(fd) {}
    ~ScopedFd() {
        if (fd_ >= 0) close(fd_);
    }
    ScopedFd(const ScopedFd&) = delete;
    ScopedFd& operator=(const ScopedFd&) = delete;
    int get() const { return fd_; }

private:
    int fd_;
};

class UtfChars {
public:
    UtfChars(JNIEnv* env, jstring value)
            : env_(env), value_(value), chars_(env->GetStringUTFChars(value, nullptr)) {}
    ~UtfChars() {
        if (chars_ != nullptr) env_->ReleaseStringUTFChars(value_, chars_);
    }
    const char* get() const { return chars_; }

private:
    JNIEnv* env_;
    jstring value_;
    const char* chars_;
};

void throwFailure(JNIEnv* env, const std::string& message) {
    jclass errorClass = env->FindClass("java/lang/IllegalStateException");
    if (errorClass != nullptr) env->ThrowNew(errorClass, message.c_str());
}

std::string errnoMessage(const char* action) {
    return std::string(action) + ": " + std::strerror(errno);
}

bool writeAll(int fd, const char* data, size_t length) {
    size_t offset = 0;
    while (offset < length) {
        const ssize_t count = write(fd, data + offset, length - offset);
        if (count < 0) {
            if (errno == EINTR) continue;
            return false;
        }
        offset += static_cast<size_t>(count);
    }
    return true;
}

bool writeMarkerFile(int directoryFd, const char* name, const char* marker, std::string* error) {
    ScopedFd file(openat(
            directoryFd,
            name,
            O_WRONLY | O_CREAT | O_TRUNC | O_CLOEXEC | O_NOFOLLOW,
            0600
    ));
    if (file.get() < 0) {
        *error = errnoMessage("openat marker");
        return false;
    }
    const size_t length = std::strlen(marker);
    if (!writeAll(file.get(), marker, length) || fsync(file.get()) != 0) {
        *error = errnoMessage("write/fsync marker");
        return false;
    }
    return true;
}

bool writeMmapFile(int directoryFd, const char* marker, std::string* error) {
    ScopedFd file(openat(
            directoryFd,
            kMmapFile,
            O_RDWR | O_CREAT | O_TRUNC | O_CLOEXEC | O_NOFOLLOW,
            0600
    ));
    if (file.get() < 0 || ftruncate(file.get(), kMmapSize) != 0) {
        *error = errnoMessage("open/ftruncate mmap marker");
        return false;
    }
    void* mapping = mmap(nullptr, kMmapSize, PROT_READ | PROT_WRITE, MAP_SHARED, file.get(), 0);
    if (mapping == MAP_FAILED) {
        *error = errnoMessage("mmap marker");
        return false;
    }
    std::memset(mapping, 0, kMmapSize);
    std::memcpy(mapping, marker, std::strlen(marker));
    const bool synced = msync(mapping, kMmapSize, MS_SYNC) == 0 && fsync(file.get()) == 0;
    const int savedErrno = errno;
    munmap(mapping, kMmapSize);
    errno = savedErrno;
    if (!synced) {
        *error = errnoMessage("msync/fsync marker");
        return false;
    }
    return true;
}

bool readMarkerFile(
        int directoryFd,
        const char* name,
        std::string* value,
        std::string* error) {
    ScopedFd file(openat(directoryFd, name, O_RDONLY | O_CLOEXEC | O_NOFOLLOW));
    if (file.get() < 0) {
        if (errno == ENOENT) {
            *value = "<missing>";
            return true;
        }
        *error = errnoMessage("openat marker");
        return false;
    }
    char buffer[kMaximumMarkerLength + 1] = {};
    struct stat fileStatus = {};
    const bool mmapFile = std::strcmp(name, kMmapFile) == 0;
    if (fstat(file.get(), &fileStatus) != 0
            || (mmapFile && fileStatus.st_size != static_cast<off_t>(kMmapSize))
            || (!mmapFile && (fileStatus.st_size <= 0
                    || fileStatus.st_size > static_cast<off_t>(kMaximumMarkerLength)))) {
        *error = "Native marker content length is invalid";
        return false;
    }
    ssize_t count;
    do {
        count = read(file.get(), buffer, sizeof(buffer));
    } while (count < 0 && errno == EINTR);
    if (count < 0) {
        *error = errnoMessage("read marker");
        return false;
    }
    size_t length = 0;
    while (length < static_cast<size_t>(count) && buffer[length] != '\0') length++;
    if (length == 0 || length > kMaximumMarkerLength
            || (!mmapFile && length != static_cast<size_t>(fileStatus.st_size))) {
        *error = "Native marker content length is invalid";
        return false;
    }
    *value = std::string(buffer, length);
    return true;
}

int openDirectory(const char* path) {
    return open(path, O_RDONLY | O_DIRECTORY | O_CLOEXEC | O_NOFOLLOW);
}
}

extern "C" JNIEXPORT void JNICALL
Java_com_uclone_slotprobe_NativeProbe_nativeWrite(
        JNIEnv* env,
        jclass,
        jstring ceDirectory,
        jstring deDirectory,
        jstring markerValue) {
    UtfChars cePath(env, ceDirectory);
    UtfChars dePath(env, deDirectory);
    UtfChars marker(env, markerValue);
    if (cePath.get() == nullptr || dePath.get() == nullptr || marker.get() == nullptr) {
        if (!env->ExceptionCheck()) throwFailure(env, "Native write received a null string");
        return;
    }
    const size_t markerLength = std::strlen(marker.get());
    if (markerLength == 0 || markerLength > kMaximumMarkerLength) {
        throwFailure(env, "Native marker length is invalid");
        return;
    }

    ScopedFd ceFd(openDirectory(cePath.get()));
    ScopedFd deFd(openDirectory(dePath.get()));
    if (ceFd.get() < 0 || deFd.get() < 0) {
        throwFailure(env, errnoMessage("open app-local marker directory"));
        return;
    }
    std::string error;
    if (!writeMarkerFile(ceFd.get(), kMarkerFile, marker.get(), &error)
            || !writeMarkerFile(deFd.get(), kMarkerFile, marker.get(), &error)
            || !writeMmapFile(ceFd.get(), marker.get(), &error)) {
        throwFailure(env, error);
    }
}

extern "C" JNIEXPORT jobjectArray JNICALL
Java_com_uclone_slotprobe_NativeProbe_nativeRead(
        JNIEnv* env,
        jclass,
        jstring ceDirectory,
        jstring deDirectory) {
    UtfChars cePath(env, ceDirectory);
    UtfChars dePath(env, deDirectory);
    if (cePath.get() == nullptr || dePath.get() == nullptr) {
        if (!env->ExceptionCheck()) throwFailure(env, "Native read received a null path");
        return nullptr;
    }
    ScopedFd ceFd(openDirectory(cePath.get()));
    ScopedFd deFd(openDirectory(dePath.get()));
    if (ceFd.get() < 0 || deFd.get() < 0) {
        throwFailure(env, errnoMessage("open app-local marker directory"));
        return nullptr;
    }

    std::string values[3];
    std::string error;
    if (!readMarkerFile(ceFd.get(), kMarkerFile, &values[0], &error)
            || !readMarkerFile(deFd.get(), kMarkerFile, &values[1], &error)
            || !readMarkerFile(ceFd.get(), kMmapFile, &values[2], &error)) {
        throwFailure(env, error);
        return nullptr;
    }
    jclass stringClass = env->FindClass("java/lang/String");
    jobjectArray result = env->NewObjectArray(3, stringClass, nullptr);
    if (result == nullptr || stringClass == nullptr) {
        if (!env->ExceptionCheck()) throwFailure(env, "Native read could not allocate response");
        return nullptr;
    }
    for (jsize index = 0; index < 3; index++) {
        jstring value = env->NewStringUTF(values[index].c_str());
        if (value == nullptr) {
            if (!env->ExceptionCheck()) throwFailure(env, "Native read could not allocate marker");
            return nullptr;
        }
        env->SetObjectArrayElement(result, index, value);
        env->DeleteLocalRef(value);
    }
    return result;
}
