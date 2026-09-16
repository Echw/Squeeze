# Jpegli WASM provenance

`web/public/wasm/jpegli.wasm` is an encoder module built locally from
[`google/jpegli`](https://github.com/google/jpegli) commit
`031a0077f5799a6041004267fc12b956c1f52a20`, including the submodule revisions
recorded by that commit. Its SHA-256 is:

```
af3581d6fa01e48d5ae3a26353b410d47185f9be28be43c6912ec6fe4c51b630
```

It was built using the official WASI SDK container
`ghcr.io/webassembly/wasi-sdk@sha256:6e11d14b5725cc6bc726b5726329bc3bfabed26879f70fbff2a025b6615a9475`.
`squeeze_wrapper.c` originates in `gen2brain/jpegli` and is adjusted for the
current Jpegli API by omitting the removed `do_fancy_downsampling` member.

Jpegli and the wrapper are BSD-3-Clause. The linked Highway SIMD library is
dual-licensed Apache-2.0 or BSD-3-Clause. Their license texts are retained
alongside this file.
