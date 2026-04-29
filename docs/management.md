# Management Strategy

## Hunk and Metadata Editing

### Uncompressed CHDs
For CHDs created with `CHD_CODEC_NONE`, the crate supports direct "on-the-fly" editing. You can use `write_hunk` or `write_bytes` to modify the container without rewriting it.

### Compressed CHDs
Modifying a hunk in a compressed CHD is non-trivial because the compressed size might change, affecting the entire file layout.

The recommended "MAME way" for on-the-fly editing of compressed CHDs is using a **Parent/Child relationship**:
1. Open the original (compressed) CHD as a **Parent**.
2. Create a new **Child** CHD (often called a "Diff" or "Delta") that points to the parent.
3. All writes are directed to the Child.
4. Reads will check the Child first; if the hunk hasn't been modified, it reads from the Parent.

This crate fully supports this via the `parent` parameter in `open` and `create`.

## Metadata Injection
Metadata can be added or deleted at any time using `write_metadata` and `delete_metadata`. MAME's core handles the complex task of shifting data blocks within the file to accommodate metadata changes.
