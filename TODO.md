# Future Work

Tracked-but-deferred features outside the current chdman parity scope.

## Deferred from chdman parity

- **GD-ROM support** (`creategd` / `extractgd`). Sega Dreamcast format with
  CHGD metadata and a split-area layout (HD area at LBA 45000). Mirror
  the eventual `cd` module's shape once it's stable. Skipped from the
  initial parity push because rusty-backup doesn't need Dreamcast on
  day one.
- **Laserdisc / AV CHDs** (`createld` / `createav` / `extractld` / `extractav`).
  Uses AVHU-coded video frames + audio. Out of scope for rusty-backup;
  add only if a downstream consumer asks.

## Other

- **CHD v3/v4 creation.** Read-only access to legacy CHDs is fine via
  `Chd::open`. We do not plan to add v3/v4 *creation* — chdman itself
  has effectively retired those paths.
- **CLI binary.** This crate intentionally stays a library. The `chdman`
  command is not being replaced; its callers are.
