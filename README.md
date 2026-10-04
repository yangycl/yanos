yanos
An x86 Kernel & Bare-Metal OS Architecture Project / x86 核心與裸機作業系統架構專案

yanos 是一個低階 x86 架構的裸機作業系統核心專案（採用 Rust 語言實作）。專案目標在於實作完整的底層硬體抽象、記憶體管理機制、核心調度器以及 PCI/USB 周邊匯流排驅動。

yanos is a low-level x86 bare-metal operating system kernel project written in Rust. The objective is to implement low-level hardware abstractions, memory management mechanisms, a kernel scheduler, and PCI/USB bus drivers.

🛠 Building & Running / 建置與執行
Dependencies / 依賴環境
Rust Toolchain: rustup (Nightly channel, x86_64-unknown-none / i686-unknown-none target)

Cargo Tools: cargo-binutils, bootimage (or custom linker scripts)

Assembler: nasm (if using external assembly entry)

Emulator: qemu-system-i386 / qemu-system-x86_64

### ELF to YEX / ELF 轉換

Repository 內的 `tools/elf2yex` 是獨立的主機端工具，可將受限的 ELF64 x86-64 executable 包裝成 yanos `.yex` 格式：

```bash
cargo run -p elf2yex -- input.elf [output.yex]
```

省略輸出路徑時會使用輸入檔名並改成 `.yex` 副檔名。此工具目前只支援單一 executable `PT_LOAD`、無 BSS 的 ELF；它不會轉換 Linux syscall 或載入動態函式庫，因此一般 Linux 執行檔不保證能在 yanos 執行。詳見 [tools/elf2yex/README.md](tools/elf2yex/README.md)。

Quick Start / 快速開始
```Bash
# Clone repository / 複製儲存庫
git clone https://github.com/yangycl/yanos.git
cd yanos

# Build kernel image / 編譯 Kernel 映像檔
cargo build --release

# Boot in QEMU / 在 QEMU 中啟動
cargo run
```
🧩 Current Architecture & Subsystems / 目前架構與子系統
Boot & Entry: #[no_std] and #[no_main] compliant bare-metal entry point, GDT/IDT initializers. / 符合 #[no_std] 與 #[no_main] 規範的裸機進入點、GDT/IDT 初始化。

Memory Management: Physical Memory Manager (PMM) & Page Table Paging (VMM) with Safe/Unsafe Abstraction Boundaries. / 物理記憶體管理器 (PMM) 與分頁頁表機制 (VMM)，包含安全與不安全記憶體抽象邊界。

Interrupt System: Exception / ISR handlers, IRQ remapping (PIC 8259A / APIC baseline). / 異常與 ISR 中斷處理常式、IRQ 重映射。

Device Drivers: VGA Textmode/Framebuffer, Serial UART (0x3F8), PS/2 Keyboard/Mouse, PCI Bus Enumerator. / VGA 文字模式、串口 UART、PS/2 鍵盤滑鼠與 PCI 匯流排列舉。

⚡ Active Development & PR Contributions / 開發重點與 PR 貢獻目標
我們正在尋求社群開發者協同開發以下模組。若你熟悉 Rust / x86 Assembly / OS Dev，歡迎直接 Issue 討論或開 Pull Request：

We are actively seeking community contributors for the following modules. If you are familiar with Rust, x86 Assembly, or OS development, feel free to open an Issue or submit a Pull Request:

High-Priority Subsystems (Help Wanted) / 高優先級子系統
USB Host Controller Driver (UHCI / EHCI)

PCI BAR 映射與 Controller 初始化 / PCI BAR mapping and controller initialization

Transfer Descriptor (TD) 與 Queue Head (QH) DMA 鏈結列實作 / TD and QH DMA linked-list implementation

USB HID Boot Protocol (Keyboard/Mouse) 數據解析與 Input Subsystem 串接 / USB HID Boot Protocol parsing and Input Subsystem integration

Virtual File System (VFS) & Initrd

VFS Node 結構與 Trait 抽象層定義 / VFS Node structure and Trait abstraction layer definition

Simple RAMDisk / TarFS 掛載實作 / Simple RAMDisk / TarFS mounting implementation

Task Scheduler & Context Switching

PCB (Process Control Block) 與 Async/Task 異步協程或搶佔式排程 / PCB design and Async/Task or Preemptive Round-Robin scheduling

Assembly switch_to 上下文切換 / Assembly switch_to context switching

Userland & System Calls

Ring 0 -> Ring 3 切換 (TSS Setup) / Ring 0 to Ring 3 switching via TSS setup

int 0x80 / syscall 系統呼叫分派器 / System call dispatcher

📬 How to Submit a Pull Request / 如何提交 Pull Request
Fork 本儲存庫並建立 Feature 分支 (git checkout -b feature/usb-uhci-driver) / Fork the repository and create a feature branch.

請確保新增程式碼通過 cargo clippy 、 cargo fmt 以及 cargo check 檢查，且不使用 std 標準庫 (#![no_std]) / Ensure code passes cargo clippy , cargo fmt and cargo check checks without using the standard library (#![no_std]).

確保 cargo build 能成功編譯且於 QEMU 無 Panic / Triple Fault / Ensure cargo build compiles cleanly without causing Panics or Triple Faults in QEMU.

發起 Pull Request 並詳細說明測試環境與變更細節 / Open a Pull Request with details on your test environment and changes.
