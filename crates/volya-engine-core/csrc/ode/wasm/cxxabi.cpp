// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// The C++ runtime ODE's code needs on wasm32, where it is built without
// exceptions or RTTI and no C++ library is linked: allocation through malloc
// (Rust's allocator, src/c_heap.rs), and traps where it would abort. No
// headers: the allocation functions are declared by the language.

typedef decltype(sizeof 0) size_t;

extern "C" void *malloc(size_t size);
extern "C" void free(void *p);

void *operator new(size_t size)
{
  void *p = malloc(size ? size : 1);
  if (!p) __builtin_trap();
  return p;
}

void *operator new[](size_t size)
{
  return operator new(size);
}

void operator delete(void *p) noexcept
{
  free(p);
}

void operator delete[](void *p) noexcept
{
  free(p);
}

void operator delete(void *p, size_t) noexcept
{
  free(p);
}

void operator delete[](void *p, size_t) noexcept
{
  free(p);
}

extern "C" void __cxa_pure_virtual()
{
  __builtin_trap();
}

// libc++'s std::__libcpp_verbose_abort (what std::vector calls where it
// would throw), by its symbol.
extern "C" [[noreturn]] void volya_libcpp_verbose_abort(const char *, ...) __asm__(
    "_ZNSt3__222__libcpp_verbose_abortEPKcz");

extern "C" void volya_libcpp_verbose_abort(const char *, ...)
{
  __builtin_trap();
}
