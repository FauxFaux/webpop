# syntax=docker/dockerfile:1

FROM debian:stable-slim AS build

RUN apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get upgrade -y \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      build-essential ca-certificates cmake git libpng-dev zlib1g-dev \
 && apt-get clean \
 && rm -rf /var/lib/apt/lists/*

# Match the repository's upstream submodule; override when that pin changes.
ARG LIBWEBP_REVISION=1844042d1ca751e62e61df7617dba19c98588339

WORKDIR /src
RUN git init . \
 && git remote add origin https://github.com/webmproject/libwebp.git \
 && git fetch --depth=1 origin "${LIBWEBP_REVISION}" \
 && git checkout --detach FETCH_HEAD \
 && test "$(git rev-parse HEAD)" = "${LIBWEBP_REVISION}"

# WEBP_LINK_STATIC prefers archives for image dependencies too. Do not use
# -static: glibc/libm stay dynamic, including for LD_PRELOAD profiling tools.
RUN test "$(dpkg --print-architecture)" = amd64 \
 && cmake -S /src -B /build \
      -DCMAKE_BUILD_TYPE=Release \
      -DCMAKE_C_COMPILER=gcc \
      -DCMAKE_C_FLAGS_RELEASE='-O3 -g -DNDEBUG -march=x86-64-v3' \
      -DCMAKE_EXPORT_COMPILE_COMMANDS=ON \
      -DBUILD_SHARED_LIBS=OFF \
      -DWEBP_LINK_STATIC=ON \
      -DWEBP_ENABLE_SIMD=ON \
      -DWEBP_USE_THREAD=ON \
      -DWEBP_NEAR_LOSSLESS=ON \
      -DCMAKE_DISABLE_FIND_PACKAGE_JPEG=TRUE \
      -DCMAKE_DISABLE_FIND_PACKAGE_TIFF=TRUE \
      -DWEBP_BUILD_CWEBP=ON \
      -DWEBP_BUILD_ANIM_UTILS=OFF \
      -DWEBP_BUILD_DWEBP=OFF \
      -DWEBP_BUILD_GIF2WEBP=OFF \
      -DWEBP_BUILD_IMG2WEBP=OFF \
      -DWEBP_BUILD_VWEBP=OFF \
      -DWEBP_BUILD_WEBPINFO=OFF \
      -DWEBP_BUILD_LIBWEBPMUX=OFF \
      -DWEBP_BUILD_WEBPMUX=OFF \
      -DWEBP_BUILD_EXTRAS=OFF \
 && cmake --build /build --target cwebp --parallel

RUN set -eux; \
    mkdir -p /out/licenses /out/build; \
    cp /build/cwebp /out/cwebp; \
    cp /src/COPYING /out/licenses/libwebp-COPYING; \
    cp /src/PATENTS /out/licenses/libwebp-PATENTS; \
    cp /src/AUTHORS /out/licenses/libwebp-AUTHORS; \
    cp /usr/share/doc/libpng-dev/copyright /out/licenses/libpng-copyright; \
    cp /usr/share/doc/zlib1g-dev/copyright /out/licenses/zlib-copyright; \
    cp /usr/share/doc/libgcc-s1/copyright /out/licenses/gcc-runtime-copyright; \
    cp /build/CMakeCache.txt /build/compile_commands.json /out/build/; \
    cp /build/CMakeFiles/cwebp.dir/link.txt /out/build/; \
    git rev-parse HEAD > /out/build/upstream-revision.txt; \
    gcc --version > /out/build/compiler.txt; \
    ld --version > /out/build/linker.txt; \
    dpkg-query -W > /out/build/packages.txt; \
    cp /etc/os-release /out/build/os-release; \
    readelf -d /out/cwebp > /out/build/dynamic.txt; \
    readelf --version-info /out/cwebp > /out/build/symbol-versions.txt; \
    ldd /out/cwebp > /out/build/ldd.txt; \
    /out/cwebp -version > /out/build/version.txt; \
    grep -q 'libc.so.6' /out/build/dynamic.txt; \
    if grep -E 'NEEDED.*lib(webp|sharpyuv|png|z\.)' /out/build/dynamic.txt; then exit 1; fi; \
    printf 'P6\n2 2\n255\nabcdefghijkl' > /tmp/smoke.ppm; \
    /out/cwebp -quiet -lossless -exact /tmp/smoke.ppm -o /tmp/smoke.webp; \
    test -s /tmp/smoke.webp; \
    cd /out; \
    sha256sum cwebp > SHA256SUMS

# Export artifacts with --output; this is not a runnable container image.
FROM scratch AS artifacts
COPY --from=build /out/ /
