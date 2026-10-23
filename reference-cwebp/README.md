# Reference cwebp

Build an optimized Linux x86-64-v3 executable from the same libwebp revision as
the repository's `upstream/` submodule. The Dockerfile fetches a clean source
checkout without using or modifying the submodule's local build products.

From the repository root:

```sh
docker buildx build --platform linux/amd64 --pull --no-cache \
  --build-arg LIBWEBP_REVISION="$(git rev-parse HEAD:upstream)" \
  --output type=local,dest=reference-cwebp/artifacts \
  reference-cwebp
reference-cwebp/artifacts/cwebp -version
reference-cwebp/artifacts/cwebp -lossless -q 75 -m 4 input.png -o output.webp
```

The base is the floating `debian:stable-slim` tag and apt packages are unpinned.
`--pull --no-cache` refreshes both the base and package installation; this is
intentionally not a reproducible build. Source remains pinned. Update the
Dockerfile's default revision when updating the submodule; the command above
always supplies the revision recorded by the current repository commit.

The executable uses GCC `-O3 -g -DNDEBUG -march=x86-64-v3`, with normal upstream
SIMD dispatch, threading support and near-lossless support enabled. No LTO,
PGO or relaxed floating-point flags are added. Threading support does not
enable the CLI's `-mt` option by default. Debug symbols are retained for
profiling, and dynamic glibc permits `LD_PRELOAD` allocator/interception tools.
Frame pointers use GCC defaults; use DWARF unwinding for stack sampling.

libwebp (including demux and libsharpyuv), libpng and zlib are linked statically.
glibc, libm and the ELF loader remain dynamic. PNG, PNM and WebP input are
supported, along with raw YUV through upstream's `-s` option. JPEG is explicitly
disabled; upstream's static CMake mode disables TIFF. The build checks that
libwebp, libsharpyuv, libpng and zlib do not appear as ELF runtime dependencies.

Build and execute on an x86-64-v3-capable CPU. Debian stable currently uses
glibc 2.41; the executable's actual minimum symbol versions are recorded in
`artifacts/build/symbol-versions.txt`. The build also exports package/toolchain
versions, compiler commands, the link command, CMake cache, runtime dependency
inspection, upstream revision and binary SHA-256. Preserve this metadata with
any committed comparison binary and saved benchmark results. See
[the comparison guidelines](../docs/BENCHMARK-THEORY.md) for matched settings.

## Distribution notices

Keep `artifacts/licenses/` with the executable when committing or distributing
it. It contains upstream's BSD license, patent grant and authors, the installed
libpng and zlib package copyright/license notices, and GCC runtime notices
(including its Runtime Library Exception). The package copyright files also
describe source files and packaging code that are not part of this executable.

This build does not embed glibc or ship its shared libraries. It therefore
avoids the glibc source/relinking materials needed when distributing a binary
that statically embeds glibc. The `scratch` stage exports only the executable,
notices and metadata; it does not redistribute the Debian build environment.
