# mame-chd-src

The subset of [MAME](https://github.com/mamedev/mame)'s source tree that
libchdman-rs compiles: the CHD core from `src/lib/util` plus the bundled
zlib, LZMA SDK, zstd, FLAC and utf8proc from `3rdparty`. Files are
unmodified copies at their upstream paths; licenses are included at their
upstream paths too.

The exact upstream version is recorded in `MAME_VERSION`. Do not edit
these files by hand. To regenerate:

    scripts/vendor-mame.sh [path-to-mame-checkout]
