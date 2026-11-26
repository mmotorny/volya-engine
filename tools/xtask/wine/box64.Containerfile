# SPDX-FileCopyrightText: 2026 Maksym Motorny
# SPDX-License-Identifier: BSD-3-Clause

# `cargo xtask wine --emulator box64`: headless 32-bit Wine on arm64 through
# box64. Wine is an x86_64 build in WoW64 mode (its 32-bit side is PE code in
# a 64-bit process), so box64 runs it all; box64 wraps the arm64 X11, Mesa
# and font libraries installed here. Wine's x86_64 ELF parts start through
# binfmt_misc, registered at run time.

# box64 built from a release (Debian has 0.3.4).
FROM debian:trixie-slim AS box64
RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      ca-certificates curl build-essential cmake python3
ARG BOX64=0.4.4
RUN curl -fsSL https://github.com/ptitSeb/box64/archive/refs/tags/v$BOX64.tar.gz | tar -xz \
    && cmake -S box64-$BOX64 -B build -DARM_DYNAREC=ON -DCMAKE_BUILD_TYPE=RelWithDebInfo \
    && cmake --build build -j1 \
    && cp build/box64 /box64
# The x86_64 libraries box64 runs emulated rather than wraps (Wine needs its
# libgcc_s), where its install puts them.
RUN cp -R box64-$BOX64/x64lib /x64lib

FROM debian:trixie-slim
RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      ca-certificates curl xz-utils xvfb xauth xdotool \
      libgl1 libgl1-mesa-dri libglx-mesa0 libegl1 libvulkan1 \
      libx11-6 libxext6 libxrender1 libxrandr2 libxi6 libxcursor1 libxcomposite1 \
      libxinerama1 libxfixes3 libxxf86vm1 libxkbcommon0 libfreetype6 libfontconfig1 \
      gcc-mingw-w64-i686 mingw-w64-i686-dev \
    && apt-get clean && rm -rf /var/lib/apt/lists/*
# Wine, amd64 WoW64 build from https://github.com/Kron4ek/Wine-Builds: the
# stable release, as development release 11.22 aborts under box64 at start
# (measured: an assertion on its mapping granularity in Wine's virtual
# memory code, with box64 0.3.4 and 0.4.4).
ARG WINE=11.0
RUN curl -fsSL https://github.com/Kron4ek/Wine-Builds/releases/download/$WINE/wine-$WINE-amd64-wow64.tar.xz \
      | tar -xJ -C /opt \
    && ln -s /opt/wine-$WINE-amd64-wow64 /opt/wine
COPY --from=box64 /box64 /usr/bin/box64
COPY --from=box64 /x64lib /usr/lib/box64-x86_64-linux-gnu
ENV PATH=/opt/wine/bin:$PATH WINEPREFIX=/root/wine WINEDLLOVERRIDES=mscoree,mshtml= WINEDEBUG=fixme-all
