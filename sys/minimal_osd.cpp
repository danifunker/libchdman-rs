#include "osdcore.h"
#include "osdfile.h"
#include <chrono>
#include <thread>
#include <mutex>
#include <vector>
#include <condition_variable>
#include <queue>
#include <atomic>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <cwchar>

#if defined(_WIN32)
#include <windows.h>
#include <direct.h>
#include <io.h>
#else
#include <unistd.h>
#include <fcntl.h>
#include <sys/stat.h>
#endif

// --- Timing ---
osd_ticks_t osd_ticks() noexcept {
    return std::chrono::high_resolution_clock::now().time_since_epoch().count();
}

osd_ticks_t osd_ticks_per_second() noexcept {
    return std::chrono::high_resolution_clock::period::den / std::chrono::high_resolution_clock::period::num;
}

void osd_sleep(osd_ticks_t duration) noexcept {
    std::this_thread::sleep_for(std::chrono::high_resolution_clock::duration(duration));
}

// --- Work Queues (Minimal) ---
struct osd_work_queue {
    std::vector<std::thread> threads;
    std::queue<osd_work_item*> items;
    std::mutex mutex;
    std::condition_variable cv;
    std::condition_variable done_cv;
    std::atomic<int> pending_items{0};
    int queue_waiters = 0;
    bool should_exit = false;

    osd_work_queue() {}
};

struct osd_work_item {
    osd_work_callback callback;
    void* param;
    void* result;
    std::atomic<bool> completed{false};
    bool waited_on = false;
    uint32_t flags;
    osd_work_queue* queue;

    osd_work_item(osd_work_queue* q, osd_work_callback cb, void* p, uint32_t f)
        : callback(cb), param(p), result(nullptr), flags(f), queue(q) {}
};

static void worker_thread(osd_work_queue* queue, int threadid) {
    while (true) {
        osd_work_item* item = nullptr;
        {
            std::unique_lock<std::mutex> lock(queue->mutex);
            queue->cv.wait(lock, [queue] { return queue->should_exit || !queue->items.empty(); });
            if (queue->should_exit && queue->items.empty()) return;
            item = queue->items.front();
            queue->items.pop();
        }
        const bool auto_release = (item->flags & WORK_ITEM_FLAG_AUTO_RELEASE) != 0;
        item->result = item->callback(item->param, threadid);
        if (auto_release) {
            delete item;
            item = nullptr;
        }
        bool wake;
        {
            std::lock_guard<std::mutex> lock(queue->mutex);
            queue->pending_items--;
            wake = (item && item->waited_on) || (queue->queue_waiters > 0 && queue->pending_items == 0);
            // Must be the last access to the item: osd_work_item_wait's
            // fast path reads `completed` without the lock, and its caller
            // may release the item as soon as it sees it.
            if (item) item->completed = true;
        }
        if (wake) queue->done_cv.notify_all();
    }
}

template <typename Pred>
static bool wait_for_ticks(osd_work_queue* queue, std::unique_lock<std::mutex>& lock, osd_ticks_t timeout, Pred done) {
    using clock = std::chrono::high_resolution_clock;
    constexpr osd_ticks_t max_timeout =
        std::chrono::duration_cast<clock::duration>(std::chrono::hours(24 * 365)).count();
    if (timeout > max_timeout) {
        queue->done_cv.wait(lock, done);
        return true;
    }
    return queue->done_cv.wait_for(lock, clock::duration(timeout), done);
}

osd_work_queue* osd_work_queue_alloc(int flags) {
    auto q = new osd_work_queue();
    int num_threads = 1;
    if (flags & WORK_QUEUE_FLAG_MULTI) {
        // Test-only override: lets the regression suite simulate a
        // high-core host on a small-core CI runner so that the cap below
        // is actually exercised. Never honored if unset (production path).
        if (const char* force = std::getenv("LIBCHDMAN_TEST_FORCE_NPROC")) {
            int forced = std::atoi(force);
            num_threads = (forced > 0) ? forced : (int)std::thread::hardware_concurrency();
        } else {
            num_threads = (int)std::thread::hardware_concurrency();
        }
        if (num_threads == 0) num_threads = 1;
    }
    if (const char* max_env = std::getenv("OSDWORKQUEUEMAXTHREADS")) {
        char* end = nullptr;
        long max_threads = std::strtol(max_env, &end, 10);
        if (end != max_env && *end == '\0' && max_threads > 0 && max_threads < num_threads)
            num_threads = (int)max_threads;
    }
    // MAME's chd_file_compressor (lib/util/chd.cpp) declares
    // chd_compressor_group *m_codecs[WORK_MAX_THREADS] (WORK_MAX_THREADS is
    // 16, from osdcore.h) and indexes it with the callback's threadid in
    // async_compress_hunk. The threadid we hand to callbacks is the worker
    // thread's index, so more than WORK_MAX_THREADS workers would run past
    // the end of that array — observed as a STATUS_STACK_BUFFER_OVERRUN
    // abort on a 20-CPU Windows host before this cap existed.
    if (num_threads > WORK_MAX_THREADS) num_threads = WORK_MAX_THREADS;
    for (int i = 0; i < num_threads; i++) {
        q->threads.emplace_back(worker_thread, q, i);
    }
    return q;
}

int osd_work_queue_items(osd_work_queue* queue) {
    return queue->pending_items;
}

bool osd_work_queue_wait(osd_work_queue* queue, osd_ticks_t timeout) {
    std::unique_lock<std::mutex> lock(queue->mutex);
    queue->queue_waiters++;
    bool done = wait_for_ticks(queue, lock, timeout, [queue] { return queue->pending_items == 0; });
    queue->queue_waiters--;
    return done;
}

void osd_work_queue_free(osd_work_queue* queue) {
    {
        std::lock_guard<std::mutex> lock(queue->mutex);
        queue->should_exit = true;
    }
    queue->cv.notify_all();
    for (auto& t : queue->threads) t.join();
    delete queue;
}

osd_work_item* osd_work_item_queue_multiple(osd_work_queue* queue, osd_work_callback callback, int32_t numitems, void* parambase, int32_t paramstep, uint32_t flags) {
    if (numitems <= 0) return nullptr;
    // Allocate before taking the lock, which every worker contends on
    if (numitems == 1) {
        auto item = new osd_work_item(queue, callback, parambase, flags);
        {
            std::lock_guard<std::mutex> lock(queue->mutex);
            queue->items.push(item);
            queue->pending_items++;
        }
        queue->cv.notify_one();
        return item;
    }
    std::vector<osd_work_item*> batch;
    batch.reserve(numitems);
    for (int i = 0; i < numitems; i++)
        batch.push_back(new osd_work_item(queue, callback, (uint8_t*)parambase + i * paramstep, flags));
    {
        std::lock_guard<std::mutex> lock(queue->mutex);
        for (osd_work_item* item : batch) queue->items.push(item);
        queue->pending_items += numitems;
    }
    queue->cv.notify_all();
    return batch.back();
}

bool osd_work_item_wait(osd_work_item* item, osd_ticks_t timeout) {
    if (item->completed) return true;
    std::unique_lock<std::mutex> lock(item->queue->mutex);
    item->waited_on = true;
    return wait_for_ticks(item->queue, lock, timeout, [item] { return item->completed.load(); });
}

void* osd_work_item_result(osd_work_item* item) {
    return item->result;
}

void osd_work_item_release(osd_work_item* item) {
    if (!item->completed) osd_work_item_wait(item, ~osd_ticks_t(0));
    delete item;
}

// --- File I/O (Minimal & Portable) ---
class minimal_osd_file : public osd_file {
public:
    FILE* f;
    minimal_osd_file(FILE* file) : f(file) {}
    ~minimal_osd_file() { if (f) fclose(f); }

    virtual std::error_condition read(void* buffer, uint64_t offset, uint32_t count, uint32_t& actual) noexcept override {
#if defined(_WIN32)
        _fseeki64(f, offset, SEEK_SET);
#else
        fseeko(f, (off_t)offset, SEEK_SET);
#endif
        size_t res = fread(buffer, 1, count, f);
        actual = (uint32_t)res;
        if (res < count && ferror(f)) return std::error_condition(errno, std::generic_category());
        return std::error_condition();
    }

    virtual std::error_condition write(const void* buffer, uint64_t offset, uint32_t count, uint32_t& actual) noexcept override {
#if defined(_WIN32)
        _fseeki64(f, offset, SEEK_SET);
#else
        fseeko(f, (off_t)offset, SEEK_SET);
#endif
        size_t res = fwrite(buffer, 1, count, f);
        actual = (uint32_t)res;
        if (res < count && ferror(f)) return std::error_condition(errno, std::generic_category());
        return std::error_condition();
    }

    virtual std::error_condition truncate(uint64_t offset) noexcept override {
#if defined(_WIN32)
        if (_chsize_s(_fileno(f), offset) != 0) return std::error_condition(errno, std::generic_category());
#else
        if (ftruncate(fileno(f), (off_t)offset) != 0) return std::error_condition(errno, std::generic_category());
#endif
        return std::error_condition();
    }

    virtual std::error_condition flush() noexcept override {
        fflush(f);
        return std::error_condition();
    }
};

std::error_condition osd_file::open(std::string const& path, uint32_t openflags, ptr& file, uint64_t& filesize) noexcept {
    const char* mode = "rb";
    if ((openflags & OPEN_FLAG_READ) && (openflags & OPEN_FLAG_WRITE)) {
        if (openflags & OPEN_FLAG_CREATE) mode = "w+b";
        else mode = "r+b";
    } else if (openflags & OPEN_FLAG_WRITE) {
        mode = "wb";
    }

    FILE* f = fopen(path.c_str(), mode);
    if (!f) return std::error_condition(errno, std::generic_category());

#if defined(_WIN32)
    _fseeki64(f, 0, SEEK_END);
    filesize = _ftelli64(f);
    _fseeki64(f, 0, SEEK_SET);
#else
    fseeko(f, 0, SEEK_END);
    filesize = ftello(f);
    fseeko(f, 0, SEEK_SET);
#endif

    file = std::make_unique<minimal_osd_file>(f);
    return std::error_condition();
}

std::error_condition osd_file::remove(std::string const& filename) noexcept {
    if (::remove(filename.c_str()) != 0) return std::error_condition(errno, std::generic_category());
    return std::error_condition();
}

std::error_condition osd_file::openpty(ptr &file, std::string &name) noexcept {
    return std::make_error_condition(std::errc::not_supported);
}

// --- Path utils ---
std::error_condition osd_get_full_path(std::string &dst, std::string const &path) noexcept {
#if defined(_WIN32)
    char full[_MAX_PATH];
    if (_fullpath(full, path.c_str(), _MAX_PATH)) {
        dst = full;
        return std::error_condition();
    }
#else
    char* resolved = realpath(path.c_str(), nullptr);
    if (resolved) {
        dst = resolved;
        free(resolved);
        return std::error_condition();
    }
#endif
    dst = path;
    return std::error_condition();
}

bool osd_is_absolute_path(std::string const &path) noexcept {
    if (path.empty()) return false;
#if defined(_WIN32)
    return (path.size() >= 3 && isalpha(path[0]) && path[1] == ':' && (path[2] == '\\' || path[2] == '/'));
#else
    return path[0] == '/';
#endif
}

// --- Logging ---
void osd_vprintf_error(util::format_argument_pack<char> const &args) {
    fputs(util::string_format(args).c_str(), stderr);
}
void osd_vprintf_warning(util::format_argument_pack<char> const &args) {}
void osd_vprintf_info(util::format_argument_pack<char> const &args) {}
void osd_vprintf_verbose(util::format_argument_pack<char> const &args) {}
void osd_vprintf_debug(util::format_argument_pack<char> const &args) {}

// --- Misc ---
const char* osd_get_bare_build_version() { return "0.287"; }
const char* osd_getenv(const char* name) { return getenv(name); }
int osd_getpid() noexcept {
#if defined(_WIN32)
    return (int)GetCurrentProcessId();
#else
    return (int)getpid();
#endif
}
void osd_break_into_debugger(const char* message) {}
std::pair<std::error_condition, unsigned> osd_get_cache_line_size() noexcept { return {std::error_condition(), 64}; }

// osd_uchar_from_osdchar is provided by mame-chd-src/src/osd/strconv.cpp on all platforms.
