// Harness for the work queue in sys/minimal_osd.cpp, run by
// scripts/osd-queue-harness.sh so it can use sanitizers.
//
//   check  worker counts, a chd.cpp-shaped stress run and timeouts; any
//          failed check or sanitizer report fails the run
//   bench  per-item cost of the same pipeline with synthetic work

#include "osdcore.h"

#include <algorithm>
#include <atomic>
#include <chrono>
#include <condition_variable>
#include <cstdio>
#include <cstdlib>
#include <mutex>
#include <string>
#include <thread>
#include <vector>

#include <sys/resource.h>

namespace {

int g_failures = 0;

void check(bool ok, const char* what) {
    if (!ok) {
        std::fprintf(stderr, "FAIL: %s\n", what);
        g_failures++;
    }
}

void set_env(const char* name, const char* value) {
    if (value) setenv(name, value, 1);
    else unsetenv(name);
}

// Worker count: queue expected + 1 items that each wait (up to 300ms) for
// expected + 1 of them to run at once. Exactly `expected` workers gives a
// peak of `expected`; more or fewer workers give a different peak.
struct probe {
    std::mutex m;
    std::condition_variable cv;
    int running = 0, peak = 0, target = 0, max_threadid = -1;
};

void* probe_cb(void* param, int threadid) {
    auto* p = static_cast<probe*>(param);
    std::unique_lock<std::mutex> lock(p->m);
    p->max_threadid = std::max(p->max_threadid, threadid);
    p->peak = std::max(p->peak, ++p->running);
    p->cv.notify_all();
    p->cv.wait_for(lock, std::chrono::milliseconds(300), [p] { return p->running >= p->target; });
    p->running--;
    return nullptr;
}

void check_workers(const char* nproc, const char* max_threads, int expected) {
    set_env("LIBCHDMAN_TEST_FORCE_NPROC", nproc);
    set_env("OSDWORKQUEUEMAXTHREADS", max_threads);
    probe p;
    p.target = expected + 1;
    osd_work_queue* q = osd_work_queue_alloc(WORK_QUEUE_FLAG_MULTI);
    for (int i = 0; i < p.target; i++) osd_work_item_queue(q, probe_cb, &p, WORK_ITEM_FLAG_AUTO_RELEASE);
    osd_work_queue_wait(q, ~osd_ticks_t(0));
    osd_work_queue_free(q);
    std::printf("  nproc=%s OSDWORKQUEUEMAXTHREADS=%s: %d workers (expected %d)\n", nproc,
                max_threads ? max_threads : "unset", p.peak, expected);
    check(p.peak == expected, "worker count");
    check(p.max_threadid < WORK_MAX_THREADS, "threadid >= WORK_MAX_THREADS");
}

// chd_file_compressor's pipeline: an IO-queue "read" item queues HALF work
// items one call at a time; the main thread consumes them in order with a
// timed wait, then releases each one. At most WINDOW items are in flight.
constexpr int WINDOW = 256, HALF = WINDOW / 2;

struct pipeline {
    osd_work_queue* work = nullptr;
    int total = 0;
    uint64_t work_ns = 0;
    std::atomic<osd_work_item*> slot[WINDOW] = {};
    int payload[WINDOW] = {};
    std::atomic<uint64_t> sum{0};
} *g_pipe;

void* work_cb(void* param, int) {
    const auto end = std::chrono::steady_clock::now() + std::chrono::nanoseconds(g_pipe->work_ns);
    while (g_pipe->work_ns && std::chrono::steady_clock::now() < end) {}
    g_pipe->sum += uint64_t(*static_cast<int*>(param)) + 1;
    return param;
}

void* read_cb(void* param, int) {
    const int first = int(reinterpret_cast<intptr_t>(param)) * HALF;
    for (int n = first; n < std::min(first + HALF, g_pipe->total); n++) {
        g_pipe->payload[n % WINDOW] = n;
        g_pipe->slot[n % WINDOW] = osd_work_item_queue(g_pipe->work, work_cb, &g_pipe->payload[n % WINDOW], 0);
    }
    return nullptr;
}

double cpu_seconds() {
    rusage ru{};
    getrusage(RUSAGE_SELF, &ru);
    return ru.ru_utime.tv_sec + ru.ru_stime.tv_sec + (ru.ru_utime.tv_usec + ru.ru_stime.tv_usec) / 1e6;
}

// Returns {wall seconds, cpu seconds}; records failures via check().
std::pair<double, double> run_pipeline(int total, uint64_t work_ns, osd_ticks_t timeout) {
    pipeline p;
    p.total = total;
    p.work_ns = work_ns;
    g_pipe = &p;
    const double cpu0 = cpu_seconds();
    const auto t0 = std::chrono::steady_clock::now();
    osd_work_queue* io = osd_work_queue_alloc(WORK_QUEUE_FLAG_IO);
    p.work = osd_work_queue_alloc(WORK_QUEUE_FLAG_MULTI);
    bool results_ok = true;
    for (int next_read = 0, done = 0; done < total;) {
        while (next_read * HALF < total && (next_read + 1) * HALF <= done + WINDOW && osd_work_queue_items(io) < 2)
            osd_work_item_queue(io, read_cb, reinterpret_cast<void*>(intptr_t(next_read++)), WORK_ITEM_FLAG_AUTO_RELEASE);
        osd_work_item* item = p.slot[done % WINDOW];
        if (!item || !osd_work_item_wait(item, timeout)) continue;  // chd.cpp spins here too
        results_ok &= osd_work_item_result(item) == &p.payload[done % WINDOW] && p.payload[done % WINDOW] == done;
        p.slot[done % WINDOW] = nullptr;
        osd_work_item_release(item);
        done++;
    }
    osd_work_queue_wait(io, ~osd_ticks_t(0));
    osd_work_queue_free(p.work);
    osd_work_queue_free(io);
    const double wall = std::chrono::duration<double>(std::chrono::steady_clock::now() - t0).count();
    check(results_ok, "item returned the wrong result");
    check(p.sum == uint64_t(total) * (total + 1) / 2, "items lost or run twice");
    return { wall, cpu_seconds() - cpu0 };
}

struct gate {
    std::mutex m;
    std::condition_variable cv;
    bool open = false;
};

void* gated_cb(void* param, int) {
    auto* g = static_cast<gate*>(param);
    std::unique_lock<std::mutex> lock(g->m);
    g->cv.wait(lock, [g] { return g->open; });
    return param;
}

void check_timeouts() {
    gate g;
    osd_work_queue* q = osd_work_queue_alloc(WORK_QUEUE_FLAG_MULTI);
    osd_work_item* item = osd_work_item_queue(q, gated_cb, &g, 0);
    check(!osd_work_item_wait(item, osd_ticks_per_second() / 100), "item wait ignored its timeout");
    check(!osd_work_queue_wait(q, osd_ticks_per_second() / 100), "queue wait ignored its timeout");
    { std::lock_guard<std::mutex> lock(g.m); g.open = true; }
    g.cv.notify_all();
    check(osd_work_item_wait(item, ~osd_ticks_t(0)), "item never completed");
    check(osd_work_queue_wait(q, ~osd_ticks_t(0)) && osd_work_queue_items(q) == 0, "queue never drained");
    osd_work_item_release(item);
    osd_work_queue_free(q);
}

} // namespace

int main(int argc, char** argv) {
    const std::string mode = argc > 1 ? argv[1] : "check";
    if (mode == "check") {
        std::printf("worker counts\n");
        check_workers("64", nullptr, WORK_MAX_THREADS);
        check_workers("64", "3", 3);
        check_workers("5", "7x", 5);
        check_workers("5", "0", 5);
        std::printf("pipeline stress\n");
        set_env("LIBCHDMAN_TEST_FORCE_NPROC", "64");
        set_env("OSDWORKQUEUEMAXTHREADS", nullptr);
        for (osd_ticks_t timeout : { osd_ticks_per_second(), osd_ticks_t(1) })
            run_pipeline(20000, 0, timeout);
        std::printf("timeouts\n");
        check_timeouts();
        std::printf(g_failures ? "%d check(s) FAILED\n" : "all checks passed\n", g_failures);
    } else if (mode == "bench") {
        std::printf("workers  work/item   wall/item   cpu/item  efficiency\n");
        for (int workers : { 1, 4, WORK_MAX_THREADS }) {
            set_env("LIBCHDMAN_TEST_FORCE_NPROC", std::to_string(workers).c_str());
            for (uint64_t work_ns : { 0, 5000, 50000 }) {
                const int items = work_ns >= 50000 ? 20000 : 200000;
                auto [wall, cpu] = run_pipeline(items, work_ns, osd_ticks_per_second());
                std::printf("%7d %8.1fus %9.0fns %9.0fns %10.0f%%\n", workers, work_ns / 1e3, wall * 1e9 / items,
                            cpu * 1e9 / items, 100.0 * items * work_ns / 1e9 / workers / wall);
            }
        }
    } else {
        std::fprintf(stderr, "usage: %s [check|bench]\n", argv[0]);
        return 2;
    }
    return g_failures ? 1 : 0;
}
