# elf2yex

`elf2yex` 是供主機 Linux 使用的 CLI 工具，將受限的 ELF64 x86-64 executable 轉成 yanos 目前使用的 `.yex` 格式：

- 4 bytes：`YEXE`
- 4 bytes：little-endian payload 長度
- 剩餘 bytes：從 ELF entry point 開始的 executable segment 內容
- YEXE 檔案最大 64 KiB，與目前核心的載入緩衝區一致

## 使用方式

在 repository 根目錄執行：

```sh
cargo run -p elf2yex -- input.elf [output.yex]
```

省略輸出路徑時，會將輸入檔副檔名改成 `.yex`。也可以先建置執行檔：

```sh
cargo build -p elf2yex --release
./target/release/elf2yex input.elf output.yex
```

## 目前限制

轉換器只接受 ELF64、little-endian、x86-64 `ET_EXEC`，且必須恰有一個 `PT_LOAD` segment。該 segment 必須可執行、沒有 BSS，entry point 必須位於 segment 的檔案內容中。其他 segment、資料區、動態連結與一般 Linux 程式所需的載入環境都不支援。

轉換只包裝 payload，不會把 Linux ABI 或 syscall 轉成 yanos ABI。yanos 現有載入器會在核心模式直接呼叫 payload，因此輸入程式必須是適用於目前執行環境的 freestanding x86-64 程式碼；一般由 gcc/clang 產生、依賴 libc/Linux syscall 或固定 ELF 載入位址的程式通常不能直接執行。這是格式轉換器的第一版，不是完整 ELF loader。
