# yanos Explorer

This is a freestanding C explorer that calls the kernel's `int 0x80` interface through x86-64 assembly functions declared in `syscalls.h`. It can list directories, move into subdirectories, go to the parent directory, and display the first 512 bytes of a selected file.

## Build

From the repository root:

```sh
make -C tools/explorer
```

The build requires NASM, GCC, GNU binutils, and Cargo. It compiles the C and assembly objects, links a position-independent single-segment ELF executable, and converts it with the workspace `elf2yex` tool. The output is `tools/explorer/build/EXPLORER.YEX`.

## Syscall ABI

The C functions are declared in `syscalls.h` and implemented in `syscalls.asm`:

- `yanos_get_key()` — syscall 3; returns one PS/2 scan code, or zero when none is queued.
- `yanos_read_file(path, path_len, destination, capacity)` — syscall 4; returns bytes copied.
- `yanos_list_directory(path, path_len, entries, capacity)` — syscall 5; writes packed 32-byte FAT directory entries and returns their count.
- `yanos_draw_text(...)` and `yanos_draw_rect(...)` — syscalls 2 and 1.

Syscalls 4 and 5 use `RBX=path`, `RCX=path length`, `RDX=destination`, and `RSI=destination capacity`. The path is relative to the mounted FAT32 root; an empty path selects the root directory.

The kernel's `SyscallRegs` field order must match the register push order in the interrupt stub. The assembly wrapper depends on `RAX` being restored from the syscall handler as the return value.
