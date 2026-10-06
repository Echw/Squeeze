#!/bin/sh
# Runs inside the WASI SDK container. /src = jpegli checkout, /work = wrapper + output.
set -eu
cd /work
cmake -S /src -B /work/cmake-build -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=0 -DBUILD_TESTING=0 \
  -DJPEGLI_ENABLE_OPENEXR=0 -DJPEGLI_ENABLE_TOOLS=0 -DJPEGLI_ENABLE_DOXYGEN=0 -DJPEGLI_ENABLE_MANPAGES=0 \
  -DJPEGLI_ENABLE_BENCHMARK=0 -DJPEGLI_ENABLE_JNI=0 -DJPEGLI_ENABLE_SJPEG=0 -DJPEGLI_ENABLE_JPEGLI_LIBJPEG=0 \
  -DJPEGLI_ENABLE_WASM_THREADS=0 -DJPEGLI_LIBJPEG_LIBRARY_SOVERSION=8 -DUSE_SIMD=ON \
  -DCMAKE_TOOLCHAIN_FILE=/work/toolchain.cmake > /work/cmake.log
cmake --build /work/cmake-build --target jpegli-static -j"$(nproc)" > /work/make.log
EXPORTS=""
for name in malloc free decode encode ${EXTRA_EXPORTS:-}; do EXPORTS="$EXPORTS -Wl,--export=$name"; done
/opt/wasi-sdk/bin/clang++ --sysroot=/opt/wasi-sdk/share/wasi-sysroot --target=wasm32-wasip1 -D__wasm_exception_handling__ -O3 \
  -Wl,--no-entry $EXPORTS -Wl,--strip-debug -mexec-model=reactor -fno-exceptions -mnontrapping-fptoint \
  -I /src -I /src/lib -I /work/cmake-build/lib/include/jpegli -Wall -o /work/jpegli.wasm \
  -x c++ /work/${WRAPPER:-squeeze_wrapper.c} -x none /work/cmake-build/lib/libjpegli-static.a /work/cmake-build/third_party/highway/libhwy.a
