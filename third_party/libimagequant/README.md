# libimagequant 2.4.1 provenance

`crates/optimizer-core/src/imagequant/` is a Rust port of libimagequant 2.4.1,
the last release of pngquant's library under the BSD-2-Clause licence (later
releases are GPL-3.0). The port follows the tree of the maintained BSD fork
[`lovell/libimagequant`](https://github.com/lovell/libimagequant) at commit
`ce5fdeb`, which is the 2.4.1 tarball plus performance patches. `COPYRIGHT`
alongside this file holds the original BSD-2-Clause and Poskanzer notices that
apply to the port.

Squeeze's changes:

- nearest-colour search no longer drops colours from later vantage-point
  heads, so it always returns the nearest palette entry;
- fixed colours, the IE6 opacity workaround, output posterisation and the
  low-memory row callback are not ported;
- image rows are converted to the internal colour space on demand instead of
  keeping a float copy of the whole image.
