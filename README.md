# nvmetoy

A toy userspace NVMe driver. It unbinds an NVMe controller from the kernel, maps
its registers through sysfs, drives admin and I/O queue pairs over a pinned
hugepage, and exposes a small REPL that reads and writes the device blocks of a
single file.

## Requirements

- Linux, run as root (register mmap, hugepages, driver unbind).
- The target NVMe unbound from the kernel `nvme` driver (the REPL refuses to touch a bound
  controller).
- Preallocated 2 MiB hugepages for the DMA region.
- `iommu=off`. With no IOMMU translation, the controller DMAs to physical addresses
  directly, which `nvmetoy` looks up from `/proc/self/pagemap`. VFIO is therefore not used.

## Running

`nvmetoy.sh` assumes the device is bound to the kernel driver with its
filesystem unmounted. It mounts the filesystem read-only just long enough to
read the file's block map, unbinds the driver, drops into the REPL, then rebinds
and leaves the device unmounted on exit:

```
sudo ./nvmetoy.sh <file> [bdf] [xfs-device]
```

`run-swissknife.sh` syncs the tree to a remote host, runs the tests, and delegates to
`nvmetoy.sh` over an SSH session with a TTY (the REPL needs one).

The REPL binary can also be run directly once the device is unbound and a block map exists:

```
cargo build --release --bin nvmetoy_repl
sudo ./target/release/nvmetoy_repl <bdf> <map-file> [nsid]
```

## REPL commands

LBAs are absolute device blocks, text is ASCII, counts are decimal. Tab completes commands
and, for the I/O commands, the file's extent-start LBAs.

```
list                     print the file's device blocks
identify                 print the Identify Controller data
read <lba> <count>       read count blocks, one block per line
write <lba> <text>       overwrite whole block(s) with text, tail zeroed
fill <lba> <char> [n]    write n blocks (default 1) of a repeated character
peek <lba> <off> <n>     read n bytes at a byte offset within a block
poke <lba> <off> <text>  patch a string at a byte offset, keeping the rest
help                     command list
quit | exit | Ctrl-D     leave the REPL
```

A single `read` or `write` is bounded by the one 2 MiB hugepage DMA buffer (about 4056
512-byte blocks).

## Layout

- `src/dma_region.rs`: the pinned hugepage, virtual/physical address pair, and bounds-checked accessors.
- `src/nvme_bar.rs`: the mmap'd register BAR and typed register accessors.
- `src/nvme_spec.rs`: register offsets, opcodes, and the queue-entry and Identify structs.
- `src/queue_pair.rs`: submission/completion queue pair, doorbells, and phase-tag polling.
- `src/nvme_command_builders.rs`, `src/nvme_identify.rs`: command construction and Identify parsing.
- `src/nvmetoy.rs`: the driver that wires it together (`NvmeUserspaceDriver`).
- `src/bin/nvmetoy_repl.rs`: the file-confined REPL.
