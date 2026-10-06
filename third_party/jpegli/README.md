# Jpegli WASM provenance

`web/public/wasm/jpegli.wasm` is an encoder module built from
[`google/jpegli`](https://github.com/google/jpegli) commit
`031a0077f5799a6041004267fc12b956c1f52a20`, including the submodule revisions
recorded by that commit. Its SHA-256 is:

```
00db15f0a1c7bc1a45a649c0e4ad0e82b1a0491135a26680ea63bfda4e06ecef
```

It was built with `build.sh` inside the official WASI SDK container:

```sh
git clone https://github.com/google/jpegli.git && cd jpegli
git checkout 031a0077f5799a6041004267fc12b956c1f52a20
git submodule update --init --depth 1
cp <squeeze>/third_party/jpegli/{build.sh,squeeze_wrapper.c,toolchain.cmake} <work>/
docker run --rm -e EXTRA_EXPORTS=transcode -v "$PWD":/src:ro -v <work>:/work \
  ghcr.io/webassembly/wasi-sdk@sha256:6e11d14b5725cc6bc726b5726329bc3bfabed26879f70fbff2a025b6615a9475 \
  sh /work/build.sh
```

The resulting binary embeds build paths, so its bytes can differ between
machines; JPEGs it encodes are identical.

`squeeze_wrapper.c` originates in `gen2brain/jpegli`. Squeeze's changes:

- the removed `do_fancy_downsampling` member is no longer set;
- greyscale input is passed as one byte per pixel through ordinary scanlines;
- `transcode` rewrites a JPEG losslessly: coefficients and quantisation tables
  are copied, Huffman tables are optimised and the scans become progressive.

Jpegli and the wrapper are BSD-3-Clause. The linked Highway SIMD library is
dual-licensed Apache-2.0 or BSD-3-Clause. Their license texts are retained
alongside this file.
