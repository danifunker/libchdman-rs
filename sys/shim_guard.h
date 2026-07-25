#ifndef CHD_SHIM_GUARD_H
#define CHD_SHIM_GUARD_H

// Exception firewall for the `extern "C"` shim surface.
//
// MAME's C++ reports some failures by throwing. `cdrom_file`'s two
// constructors `throw nullptr` on bad input (cdrom.cpp:140, 156, 162, 253,
// 255, 260) — a hard-disk CHD trips `unit_bytes() != FRAME_SIZE` on the
// very first check — and anything that allocates can throw `std::bad_alloc`.
//
// Rust frames cannot unwind a foreign exception. Letting one cross an
// `extern "C"` boundary kills the process outright:
//
//     libc++abi: terminating due to uncaught exception of type std::nullptr_t
//     fatal runtime error: Rust cannot catch foreign exceptions, aborting
//
// So **no `extern "C"` function in this directory may let an exception
// reach its caller**. Every entry point routes its body through one of the
// two helpers below; a newly added entry point is exception-safe by
// construction if it follows the same shape, and `grep guard sys/*.cpp`
// shows the coverage.
//
// Usage — value-returning:
//
//     chd_error_t chd_shim_foo(chd_file_t* chd) {
//         return chd_shim::guard([&] { return ...; }, SHIM_ERR_EXCEPTION);
//     }
//
// void-returning:
//
//     void chd_shim_bar(chd_file_t* chd) {
//         chd_shim::guard_void([&] { ...; });
//     }
//
// `throw nullptr` carries no payload, so `catch (...)` is the only option
// and the fallback value is all the diagnosis a caller gets. Where the
// reason is knowable up front (e.g. "this CHD is not CD media"), the Rust
// side pre-screens instead of relying on the fallback — see
// `cd::Cdrom::open`.
//
// Anything allocated inside the lambda must be owned by an RAII holder so a
// throw part-way through doesn't leak; hand ownership to the caller with
// `release()` on the success path.

#include <utility>

namespace chd_shim {

namespace detail {
// Parks a type behind a dependent name so it can't take part in template
// argument deduction. std::type_identity is the standard spelling but is
// C++20-only; this header stays usable at any standard the shim is built at.
template <typename T> struct identity { using type = T; };
template <typename T> using identity_t = typename identity<T>::type;
template <typename F> using result_of_t = decltype(std::declval<F&>()());
} // namespace detail

// Run `fn`; if it throws, swallow the exception and return `fallback`.
// The fallback parameter is a non-deduced context, so plain literals
// (`0`, `nullptr`) convert to the callable's return type.
template <typename F>
detail::result_of_t<F> guard(
    F&& fn,
    detail::identity_t<detail::result_of_t<F>> fallback) noexcept {
    try {
        return fn();
    } catch (...) {
        return fallback;
    }
}

// Run `fn`; if it throws, swallow the exception. For entry points that
// return void and therefore have no way to report the failure.
template <typename F>
void guard_void(F&& fn) noexcept {
    try {
        fn();
    } catch (...) {
    }
}

} // namespace chd_shim

#endif
